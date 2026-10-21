//! Entry point for agent runs started from conversations: opens a turn,
//! records human decisions on approvals, and hands runs to the executor in
//! the background.

use std::sync::Arc;

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::approvals::{ApprovalRow, get_approval};
use mimorya_db::repositories::runs::RunRow;
use uuid::Uuid;

use super::approval_decision::{
    DecisionInput, decidable, record_approval_decision, resume_decisions,
};
use super::approval_routing::may_decide_for;
use super::conversation_turn::{open_conversation_turn, tenant_autonomy_level};
use super::executor::{ExecutionMode, RunExecutor};
use crate::services::access::{AccessContext, AccessService, AuthIdentity};
use crate::services::background::BackgroundTasks;
use crate::services::llm::Unusable;

const REQUESTER_INACTIVE: &str = "Le compte de la personne pour qui l’agent agit n’est plus \
    actif : la demande ne peut plus être traitée.";
const NO_LONGER_ENTITLED: &str =
    "Vous n’avez plus le droit de valider les actions de cette personne.";

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
        let decider = self.access.context(identity).await?;
        let mut tx = self.db.begin(identity.tenant_id).await?;
        let approval = decidable(
            get_approval(&mut tx, input.approval_id).await?,
            identity.user_id,
            input.approved,
        )?;
        tx.commit().await?;
        // The run goes on for whom it acts for, with their rights as they
        // stand now: never with the decider's (ADR-059).
        let requester = self
            .access
            .active_context(identity.tenant_id, approval.requested_for_user_id)
            .await?
            .ok_or_else(|| DomainError::conflict(REQUESTER_INACTIVE))?;
        let withdrawing = identity.user_id == approval.requested_for_user_id;
        if approval.routed_to_manager() && !withdrawing {
            // Named when the request was made, the manager still has to
            // hold `approval:decide` over the requester. Checked before the
            // decision is recorded: a right lost in between still binds the
            // run, which reads the requester's rights at each step.
            let unit = requester.profile.primary_unit_id;
            if !may_decide_for(&decider, approval.requested_for_user_id, unit) {
                return Err(DomainError::forbidden(NO_LONGER_ENTITLED).into());
            }
        } else {
            // Deciding, or withdrawing, resumes the agent: whoever may no
            // longer use it leaves the request as it is.
            decider.require("agent:use")?;
        }
        let mut tx = self.db.begin(identity.tenant_id).await?;
        let recorded = record_approval_decision(&mut tx, identity, &input, Utc::now()).await?;
        tx.commit().await?;

        let resumed = recorded.resume.is_some();
        if let Some((run, approvals)) = recorded.resume {
            self.resume(requester, run, &approvals);
        }
        match recorded.decided {
            Some(approval) if !recorded.expired => Ok(DecisionResult { approval, resumed }),
            _ => Err(DomainError::conflict("Cette demande de validation a expiré").into()),
        }
    }

    /// Resumes in the background a run its requests no longer hold, with the
    /// rights of whom it acts for (`access`) and how each request ended.
    pub(super) fn resume(&self, access: AccessContext, run: RunRow, approvals: &[ApprovalRow]) {
        let mode = ExecutionMode::Resume {
            state: run.state.clone().unwrap_or_default(),
            decisions: resume_decisions(approvals),
        };
        let executor = self.executor.clone();
        let access = Arc::new(access);
        self.tasks
            .spawn(async move { executor.execute(access, run, mode).await });
    }
}
