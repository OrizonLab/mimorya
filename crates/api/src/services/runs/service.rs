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

pub struct StartedTurn {
    pub run_id: Uuid,
    pub message_id: Uuid,
}

pub struct DecisionResult {
    pub approval: ApprovalRow,
    pub resumed: bool,
}

pub struct RunService {
    db: Database,
    access: Arc<AccessService>,
    executor: Arc<RunExecutor>,
    tasks: Arc<BackgroundTasks>,
}

impl RunService {
    pub fn new(db: Database, access: Arc<AccessService>, executor: RunExecutor) -> Self {
        Self {
            db,
            access,
            executor: Arc::new(executor),
            tasks: Arc::new(BackgroundTasks::default()),
        }
    }

    pub fn llm_available(&self) -> bool {
        self.executor.router.is_some()
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
        if !self.llm_available() {
            return Err(DomainError::unavailable(
                "Aucun fournisseur de modèle n’est configuré. Renseignez LLM_PROVIDER et la clé correspondante.",
            )
            .into());
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
