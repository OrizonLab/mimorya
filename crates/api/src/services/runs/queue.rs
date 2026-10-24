//! The run queue (ADR-085). A run set going (a turn, a decision) executes
//! in this process, under its lease (ADR-083), or, in an `api` process,
//! waits queued for a worker, woken up as the transaction commits. A worker
//! claims each queued run in its company, then executes it as it would
//! have: from its conversation for a new turn, from its saved step with how
//! its requests ended for a resumption. One that waited too long ends
//! instead, as one a stop interrupted for too long (ADR-053).

use std::sync::Arc;

use chrono::Utc;
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, RunCheckpoint, RunEvent};
use mimorya_db::TenantTx;
use mimorya_db::notifications::notify_run_queued;
use mimorya_db::repositories::approvals::list_approvals_for_run;
use mimorya_db::repositories::run_queue::{ClaimedRun, claim_queued_run, queue_run, queued_runs};
use mimorya_db::repositories::runs::InterruptedRun;
use tokio::sync::watch;
use uuid::Uuid;

use super::approval_decision::resume_decisions;
use super::conversation_turn::queued_turn_history;
use super::db_ports::RunScope;
use super::events::append_in;
use super::executor::ExecutionMode;
use super::run_access::stopped_by;
use super::service::RunService;
use crate::services::event_bus::BusEvent;

/// Why a run queued for too long ends: its request may be outdated.
pub const QUEUED_TOO_LONG: &str = "Demande restée en file plus d’une heure sans être \
    exécutée : elle n’est pas traitée. Relancez-la si elle vaut toujours.";
/// Why a queued turn whose request cannot be found again ends.
pub const QUEUED_REQUEST_NOT_FOUND: &str =
    "La demande en file n’a pas pu être retrouvée : relancez-la si elle vaut toujours.";

/// Beyond this, a queued run is not executed: its user has moved on.
const MAX_WAIT: chrono::Duration = chrono::Duration::hours(1);
/// Queued runs claimed in one go, at most.
const BATCH: i32 = 100;

/// Where a run set going executes.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Dispatch {
    /// Here, under this executor's lease: to launch once committed.
    Here,
    /// By a worker, which claims it from the queue: its `run_queued` event
    /// to broadcast here once committed.
    Queued(BusEvent),
}

impl RunService {
    /// Sets `run_id`, just set running, going in the same transaction:
    /// under this executor's lease, or queued for a worker in an `api`
    /// process, its stream saying so.
    pub(super) async fn dispatch(
        &self,
        tx: &mut TenantTx,
        run_id: Uuid,
    ) -> anyhow::Result<Dispatch> {
        if !self.queues {
            self.hold(tx, run_id).await?;
            return Ok(Dispatch::Here);
        }
        let queued = queue_run(tx, run_id).await?;
        anyhow::ensure!(queued, "run {run_id} is not running: not queued");
        let event = append_in(tx, &self.executor.bus, run_id, RunEvent::RunQueued {}).await?;
        notify_run_queued(&mut **tx).await?;
        Ok(Dispatch::Queued(event))
    }

    /// Once its transaction committed: what `dispatch` decided carried out.
    pub(super) fn dispatched(&self, run_id: Uuid, dispatch: Dispatch, launch: impl FnOnce()) {
        match dispatch {
            Dispatch::Here => launch(),
            Dispatch::Queued(event) => self.executor.bus.publish(run_id, event),
        }
    }

    /// Claims and executes the queued runs, batch after batch while some
    /// remain, in a process that executes runs only, each run claimed by one
    /// of the workers alone (ADR-086), until the service stops (`stopping`),
    /// between two runs; how many it claimed.
    pub async fn take_queued(&self, stopping: &watch::Sender<bool>) -> anyhow::Result<usize> {
        if !self.executes() {
            return Ok(0);
        }
        let mut taken = 0;
        loop {
            let queued = queued_runs(self.db.pool(), BATCH).await?;
            let mut claimed = false;
            for run in &queued {
                if *stopping.borrow() {
                    return Ok(taken);
                }
                match self.take_queued_run(*run).await {
                    Ok(true) => {
                        taken += 1;
                        claimed = true;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::error!(run_id = %run.run_id, error = format!("{error:#}"), "Queued run not taken");
                    }
                }
            }
            // A full batch: more may wait. One where none could be claimed
            // is left to the next pass.
            if queued.len() < usize::try_from(BATCH)? || !claimed {
                return Ok(taken);
            }
        }
    }

    /// Claims the run, if still queued, and executes it, or ends it; whether
    /// it was claimed.
    async fn take_queued_run(&self, queued: InterruptedRun) -> anyhow::Result<bool> {
        let InterruptedRun { tenant_id, run_id } = queued;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(ClaimedRun { run, queued_since }) = claim_queued_run(&mut tx, run_id).await?
        else {
            return Ok(false);
        };
        self.hold(&mut tx, run_id).await?;
        let scope = RunScope {
            tenant_id,
            run_id,
            user_id: run.on_behalf_of_user_id,
            conversation_id: run.conversation_id,
            mission_id: run.mission_id,
        };
        let checkpoint: Option<RunCheckpoint> = run
            .state
            .clone()
            .and_then(|state| serde_json::from_value(state).ok());
        let mode = if Utc::now() - queued_since > MAX_WAIT {
            Err(QUEUED_TOO_LONG)
        } else {
            match run.state.clone() {
                // Never executed: a new turn.
                None => queued_turn_history(&mut tx, &run)
                    .await?
                    .map(|(history, text)| ExecutionMode::Start { history, text })
                    .ok_or(QUEUED_REQUEST_NOT_FOUND),
                // Paused at a step: its requests ended since.
                Some(state) => {
                    let approvals = list_approvals_for_run(&mut tx, run_id).await?;
                    Ok(ExecutionMode::Resume {
                        state,
                        decisions: resume_decisions(&approvals),
                    })
                }
            }
        };
        tx.commit().await?;

        let mode = match mode {
            Ok(mode) => mode,
            Err(error) => {
                self.end(scope, error, checkpoint).await?;
                return Ok(true);
            }
        };
        // With the rights of whom it acts for, as they stand now: checked as
        // before each step, before anything is started for the run.
        let access = self.access.active_context(tenant_id, scope.user_id).await?;
        match (stopped_by(access.as_ref()), access) {
            (None, Some(access)) => self.launch(Arc::new(access), run, mode),
            (why, _) => {
                let why = why.unwrap_or(ACCOUNT_INACTIVE);
                self.end(scope, why, checkpoint).await?;
            }
        }
        Ok(true)
    }
}
