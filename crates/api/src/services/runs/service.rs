//! Entry point for agent runs started from conversations: opens a turn,
//! records human decisions on approvals, and hands runs to the executor in
//! the background.

use std::sync::Arc;

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::approvals::ApprovalRow;
use uuid::Uuid;

use super::approval_decision::{DecisionInput, record_approval_decision, resume_decisions};
use super::conversation_turn::{open_conversation_turn, tenant_autonomy_level};
use super::executor::{ExecutionMode, RunExecutor};
use crate::services::access::{AccessService, AuthIdentity};
use crate::services::background::BackgroundTasks;
use crate::services::llm::Unusable;

pub struct StartedTurn {
    pub run_id: Uuid,
    pub message_id: Uuid,
}

pub struct DecisionResult {
    pub approval: ApprovalRow,
    pub resumed: bool,
}

pub struct RunService {
    pub(super) db: Database,
    pub(super) access: Arc<AccessService>,
    pub(super) executor: Arc<RunExecutor>,
    pub(super) tasks: Arc<BackgroundTasks>,
    /// The run executor's lock, once this process holds it (ADR-053).
    pub(super) executor_lock: tokio::sync::Mutex<Option<sqlx::PgConnection>>,
}

impl RunService {
    pub fn new(db: Database, access: Arc<AccessService>, executor: RunExecutor) -> Self {
        Self {
            db,
            access,
            executor: Arc::new(executor),
            tasks: Arc::new(BackgroundTasks::default()),
            executor_lock: tokio::sync::Mutex::new(None),
        }
    }

    /// Resolves when no run is executing in the background (tests, shutdown).
    pub async fn idle(&self) {
        self.tasks.idle().await;
    }

    pub async fn start_conversation_turn(
        &self,
        identity: AuthIdentity,
        conversation_id: Uuid,
        text: &str,
    ) -> anyhow::Result<StartedTurn> {
        match self.executor.llm.provider_for(identity.tenant_id).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Err(DomainError::unavailable(
                    "Aucun modèle n’est disponible : la plateforme n’en fournit pas et votre \
                     entreprise n’en a pas configuré (Administration › Modèle d’IA).",
                )
                .into());
            }
            Err(error) if !error.is::<Unusable>() => return Err(error),
            Err(_) => {
                return Err(DomainError::unavailable(
                    "Le modèle choisi par votre entreprise ne peut pas être utilisé : un \
                     administrateur doit revoir sa configuration (Administration › Modèle d’IA).",
                )
                .into());
            }
        }
        let access = self.access.context(identity).await?;
        access.require("agent:use")?;
        let autonomy = tenant_autonomy_level(&access);

        let mut tx = self.db.begin(identity.tenant_id).await?;
        let turn =
            open_conversation_turn(&mut tx, identity, conversation_id, text, autonomy).await?;
        tx.commit().await?;

        let started = StartedTurn {
            run_id: turn.run.id,
            message_id: turn.user_message_id,
        };
        let executor = self.executor.clone();
        let mode = ExecutionMode::Start {
            history: turn.history,
            text: text.to_string(),
        };
        let access = Arc::new(access);
        self.tasks
            .spawn(async move { executor.execute(access, turn.run, mode).await });
        Ok(started)
    }

    pub async fn decide(
        &self,
        identity: AuthIdentity,
        input: DecisionInput,
    ) -> anyhow::Result<DecisionResult> {
        let access = self.access.context(identity).await?;
        // Deciding resumes the agent: whoever may no longer use it leaves the
        // request to expire.
        access.require("agent:use")?;
        let mut tx = self.db.begin(identity.tenant_id).await?;
        let recorded = record_approval_decision(&mut tx, identity, &input, Utc::now()).await?;
        tx.commit().await?;

        let resumed = recorded.resume.is_some();
        if let Some((run, approvals)) = recorded.resume {
            let mode = ExecutionMode::Resume {
                state: run.state.clone().unwrap_or_default(),
                decisions: resume_decisions(&approvals),
            };
            let executor = self.executor.clone();
            let access = Arc::new(access);
            self.tasks
                .spawn(async move { executor.execute(access, run, mode).await });
        }
        match recorded.decided {
            Some(approval) if !recorded.expired => Ok(DecisionResult { approval, resumed }),
            _ => Err(DomainError::conflict("Cette demande de validation a expiré").into()),
        }
    }
}
