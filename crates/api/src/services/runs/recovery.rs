//! Runs a stop of the service left running (ADR-053). At startup, before the
//! API serves, each of them was interrupted: a single instance executes runs
//! (ADR-016). Each is taken up again in its own company, from its last saved
//! step, by its executor; the engine decides whether going on is safe. The
//! validation requests of the lost step are cancelled first (the run asks
//! again if it needs to). A run stopped before its first saved step, or
//! whose user may no longer act, ends there.

use std::sync::Arc;

use mimorya_agent::runtime::{ACCOUNT_INACTIVE, RunCheckpoint, RunEvent, RunOutcome};
use mimorya_db::repositories::approvals::cancel_pending_approvals;
use mimorya_db::repositories::run_events::tool_calls_since;
use mimorya_db::repositories::runs::{InterruptedRun, RunStatus, interrupted_runs, lock_run};

use super::db_ports::RunScope;
use super::events::record_run_event;
use super::executor::ExecutionMode;
use super::outcome::persist_outcome;
use super::service::RunService;

/// Why a run stopped before its first saved step ends: nothing ran yet.
pub const STOPPED_BEFORE_ITS_FIRST_STEP: &str = "Exécution interrompue par un arrêt du service avant sa première étape : relancez votre demande.";

impl RunService {
    /// Takes up again the runs a stop of the service left running; how many
    /// there were. Called once, at startup, before serving.
    pub async fn recover_interrupted(&self) -> anyhow::Result<usize> {
        let runs = interrupted_runs(self.db.pool()).await?;
        for run in &runs {
            // One that cannot be taken up stays running: the periodic sweep
            // fails it later.
            if let Err(error) = self.recover(*run).await {
                tracing::error!(run_id = %run.run_id, error = format!("{error:#}"), "Interrupted run not taken up");
            }
        }
        Ok(runs.len())
    }

    async fn recover(&self, interrupted: InterruptedRun) -> anyhow::Result<()> {
        let InterruptedRun { tenant_id, run_id } = interrupted;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(run) = lock_run(&mut tx, run_id).await? else {
            return Ok(());
        };
        if run.status() != RunStatus::Running {
            return Ok(());
        }
        let started = tool_calls_since(&mut tx, run_id, run.updated_at).await?;
        cancel_pending_approvals(&mut tx, run_id).await?;
        tx.commit().await?;

        let scope = RunScope {
            tenant_id,
            run_id,
            user_id: run.on_behalf_of_user_id,
            conversation_id: run.conversation_id,
        };
        let Some(state) = run.state.clone() else {
            return self.end(scope, STOPPED_BEFORE_ITS_FIRST_STEP, None).await;
        };
        let Some(access) = self.access.active_context(tenant_id, scope.user_id).await? else {
            let checkpoint = serde_json::from_value(state).ok();
            return self.end(scope, ACCOUNT_INACTIVE, checkpoint).await;
        };
        let mode = ExecutionMode::Recover {
            state,
            interrupted: started,
        };
        let executor = self.executor.clone();
        let access = Arc::new(access);
        self.tasks
            .spawn(async move { executor.execute(access, run, mode).await });
        Ok(())
    }

    /// Ends a run that does not go on, as the engine would: its outcome saved
    /// (status, message in the conversation, audit), then announced.
    async fn end(
        &self,
        scope: RunScope,
        error: &str,
        checkpoint: Option<RunCheckpoint>,
    ) -> anyhow::Result<()> {
        let checkpoint =
            checkpoint.unwrap_or_else(|| RunCheckpoint::new(String::new(), Vec::new()));
        let outcome = RunOutcome::Failed {
            error: error.into(),
            checkpoint,
        };
        persist_outcome(&self.db, scope, &outcome).await?;
        let finished = RunEvent::RunFinished {
            status: outcome.status().into(),
            error: Some(error.into()),
        };
        let bus = &self.executor.bus;
        record_run_event(&self.db, bus, scope.tenant_id, scope.run_id, finished).await
    }
}
