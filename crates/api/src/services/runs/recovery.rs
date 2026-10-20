//! Runs a stop of the service left running (ADR-053). At startup, once the
//! API listens, the process that holds the run executor's lock takes each of
//! them up again: a single instance executes runs (ADR-016), so each was
//! interrupted. A second process finds the lock held and takes nothing up.
//! Each run goes on in its own company, from its last saved step, through
//! its executor; the engine decides whether going on is safe. The validation
//! requests of the lost step are cancelled first, and audited (the run asks
//! again if it needs to). A run stopped before its first saved step, for too
//! long, or whose user may no longer act, ends there.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, RunCheckpoint, RunEvent, RunOutcome};
use mimorya_db::TenantTx;
use mimorya_db::repositories::approvals::{cancel_pending_approvals, list_approvals_for_run};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::run_events::tool_calls_since;
use mimorya_db::repositories::runs::{
    InterruptedRun, RunRow, RunStatus, interrupted_runs, lock_run,
};
use sqlx::PgConnection;

use super::db_ports::RunScope;
use super::events::record_run_event;
use super::executor::ExecutionMode;
use super::outcome::persist_outcome;
use super::service::RunService;

/// Why a run stopped before its first saved step ends: nothing ran yet.
pub const STOPPED_BEFORE_ITS_FIRST_STEP: &str = "Exécution interrompue par un arrêt du service avant sa première étape : relancez votre demande.";

/// Why a run interrupted too long ago ends: its request may be outdated.
pub const INTERRUPTED_TOO_LONG: &str = "Exécution interrompue par un arrêt du service depuis plus d’une heure : elle n’est pas reprise. Relancez votre demande si elle vaut toujours.";

/// Beyond this, a run is not taken up: its user has moved on.
const MAX_INTERRUPTION: chrono::Duration = chrono::Duration::hours(1);

/// Held, for the life of the process, by the instance that takes runs up:
/// `pg_try_advisory_lock` on a connection of its own.
const RUN_EXECUTOR_LOCK: i64 = 0x6d69_6d6f_7279_6101;

/// A process that just stopped may hold the lock for a moment more.
const LOCK_ATTEMPTS: u32 = 6;
const LOCK_RETRY: Duration = Duration::from_millis(500);

impl RunService {
    /// Takes up again the runs a stop of the service left running; how many
    /// there were. Called once, at startup, once the API listens; nothing is
    /// taken up while another process holds the run executor's lock.
    pub async fn recover_interrupted(&self) -> anyhow::Result<usize> {
        if !self.hold_executor_lock().await? {
            tracing::error!(
                "Another API process holds the run executor's lock: interrupted runs are not taken up (a single instance executes runs, ADR-016)"
            );
            return Ok(0);
        }
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

    /// Takes the run executor's lock for the life of this process, unless
    /// another one holds it.
    async fn hold_executor_lock(&self) -> anyhow::Result<bool> {
        let mut held = self.executor_lock.lock().await;
        if held.is_some() {
            return Ok(true);
        }
        let mut connection: PgConnection = self.db.pool().acquire().await?.detach();
        for attempt in 1..=LOCK_ATTEMPTS {
            let taken: bool = sqlx::query_scalar("select pg_try_advisory_lock($1)")
                .bind(RUN_EXECUTOR_LOCK)
                .fetch_one(&mut connection)
                .await?;
            if taken {
                *held = Some(connection);
                return Ok(true);
            }
            if attempt < LOCK_ATTEMPTS {
                tokio::time::sleep(LOCK_RETRY).await;
            }
        }
        Ok(false)
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
        let checkpoint: Option<RunCheckpoint> = run
            .state
            .clone()
            .and_then(|state| serde_json::from_value(state).ok());
        let started = started_since_saved(&mut tx, &run, checkpoint.as_ref()).await?;
        cancel_lost_validations(&mut tx, &run).await?;
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
        if Utc::now() - run.updated_at > MAX_INTERRUPTION {
            return self.end(scope, INTERRUPTED_TOO_LONG, checkpoint).await;
        }
        let Some(access) = self.access.active_context(tenant_id, scope.user_id).await? else {
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

/// The tools a stop may have left half done: the calls started after the
/// saved step, or, when approved actions were running, those approved.
async fn started_since_saved(
    tx: &mut TenantTx,
    run: &RunRow,
    checkpoint: Option<&RunCheckpoint>,
) -> anyhow::Result<Vec<String>> {
    if let Some(pending) = checkpoint.and_then(|c| c.pending.as_ref()) {
        let approvals = list_approvals_for_run(tx, run.id).await?;
        let approved = pending
            .approvals
            .iter()
            .filter(|p| {
                approvals
                    .iter()
                    .any(|a| a.id == p.approval_id && a.status == "approved")
            })
            .map(|p| p.tool_name.clone())
            .collect();
        return Ok(approved);
    }
    // Without a mark (a state saved before it existed), every call counts.
    let after = run.checkpoint_seq.unwrap_or(0);
    Ok(tool_calls_since(tx, run.id, after).await?)
}

/// The lost step's validation requests, cancelled and audited.
async fn cancel_lost_validations(tx: &mut TenantTx, run: &RunRow) -> anyhow::Result<()> {
    for approval in cancel_pending_approvals(tx, run.id).await? {
        let entry = AuditEntry::new(run.tenant_id, "system", "approval.cancelled", "success")
            .on_behalf_of(approval.requested_for_user_id)
            .run(run.id)
            .resource("approval", approval.id)
            .reason("Étape perdue lors d’un arrêt du service");
        append_audit(tx, entry).await?;
    }
    Ok(())
}
