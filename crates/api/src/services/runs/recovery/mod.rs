//! Runs a stop of the service left running (ADR-053). At startup, once the
//! API listens and holds the run executor's lock, it takes each of them up
//! again: a single instance executes runs (ADR-016), so each was
//! interrupted. Each run goes on in its own company, from its last saved
//! step, through its executor; the engine decides whether going on is safe.
//! The validation requests of the lost step are cancelled first, and audited
//! (the run asks again if it needs to). A run stopped before its first saved
//! step, for too long, or whose user may no longer act, ends there, naming
//! the actions that were under way when the service stopped. Beforehand,
//! managers and scheduled tasks' authors are told of the requests whose
//! notice the stop lost (ADR-070, ADR-082).

mod calls;
mod lock;

use std::sync::Arc;

use chrono::DateTime;
use chrono::Utc;
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, RunCheckpoint, RunEvent, RunOutcome};
use mimorya_db::repositories::runs::{InterruptedRun, RunStatus, interrupted_runs, lock_run};

use super::db_ports::RunScope;
use super::events::record_run_event;
use super::executor::ExecutionMode;
use super::outcome::{CRASH_MESSAGE, persist_outcome};
use super::service::RunService;
use calls::{
    cancel_lost_validations, ending, of_unknown_effect, risks_set_now, started_since_saved,
};
pub use lock::LOCK_HELD_ELSEWHERE;

/// Why a run stopped before its first saved step ends: nothing ran yet.
pub const STOPPED_BEFORE_ITS_FIRST_STEP: &str = "Exécution interrompue par un arrêt du service avant sa première étape : relancez votre demande.";

/// Why a run interrupted too long ago ends: its request may be outdated.
pub const INTERRUPTED_TOO_LONG: &str = "Exécution interrompue par un arrêt du service depuis plus d’une heure : elle n’est pas reprise. Relancez votre demande si elle vaut toujours.";
const TOO_LONG: &str = "Exécution interrompue par un arrêt du service depuis plus d’une heure : elle n’est pas reprise.";

/// Shown, with the actions to check, when a run taken up cannot go on.
const NOT_TAKEN_UP: &str =
    "L’exécution, interrompue par un arrêt du service, n’a pas pu reprendre.";

/// Beyond this, a run is not taken up: its user has moved on.
const MAX_INTERRUPTION: chrono::Duration = chrono::Duration::hours(1);

/// How a run is taken up: at startup, its executor stopped (this process
/// alone executes, ADR-053), whatever its lease; or its lease expired by
/// then, and still is once locked (ADR-083).
#[derive(Debug, Clone, Copy)]
pub(super) enum TakeUp {
    AtStartup,
    Expired(DateTime<Utc>),
}

impl RunService {
    /// Takes up again the runs a stop of the service left running; how many
    /// there were. Called once, at startup, once the API listens: it takes
    /// the run executor's lock first, and fails while another process holds
    /// it. The notices the stop lost go first, before any run can pause
    /// again; failing, they leave the runs to take up.
    pub async fn recover_interrupted(&self) -> anyhow::Result<usize> {
        self.claim_execution().await?;
        let taken_up = self.take_up_interrupted().await;
        // Over, whatever came of it: a run started from now on cannot be
        // taken for one the stop interrupted.
        self.taken_up.send_replace(true);
        taken_up
    }

    /// Resolves once this process has taken up the runs a stop interrupted:
    /// it alone may then start runs nobody asked for in a request (scheduled
    /// tasks, ADR-080), none of which can be taken for one of them.
    pub async fn interrupted_runs_taken_up(&self) {
        let mut taken_up = self.taken_up.subscribe();
        // The sender lives as long as `self`: this ends only once taken up.
        let _ = taken_up.wait_for(|done| *done).await;
    }

    async fn take_up_interrupted(&self) -> anyhow::Result<usize> {
        match self.executor.notifier.catch_up(Utc::now()).await {
            Ok(0) => {}
            Ok(lost) => tracing::warn!(
                lost,
                "Managers and scheduled tasks' authors told of requests a stop kept from them"
            ),
            Err(error) => tracing::error!(
                error = format!("{error:#}"),
                "Lost approval notices not caught up"
            ),
        }
        let runs = interrupted_runs(self.db.pool()).await?;
        for run in &runs {
            // One that cannot be taken up stays running: the periodic sweep
            // fails it later.
            if let Err(error) = self.recover(*run, TakeUp::AtStartup).await {
                tracing::error!(run_id = %run.run_id, error = format!("{error:#}"), "Interrupted run not taken up");
            }
        }
        Ok(runs.len())
    }

    /// Takes the run up, under its lock and lease: resumed, or ended naming
    /// the actions to check; whether it was taken (it may have ended, or its
    /// lease been renewed, meanwhile).
    pub(super) async fn recover(
        &self,
        interrupted: InterruptedRun,
        how: TakeUp,
    ) -> anyhow::Result<bool> {
        let InterruptedRun { tenant_id, run_id } = interrupted;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(run) = lock_run(&mut tx, run_id).await? else {
            return Ok(false);
        };
        if run.status() != RunStatus::Running {
            return Ok(false);
        }
        if let TakeUp::Expired(now) = how {
            let expired = run.lease_expires_at.is_some_and(|at| at < now);
            if !expired || self.leases.executes(run_id) {
                return Ok(false);
            }
        }
        self.hold(&mut tx, run_id).await?;
        let checkpoint: Option<RunCheckpoint> = run
            .state
            .clone()
            .and_then(|state| serde_json::from_value(state).ok());
        let (started, approved) = started_since_saved(&mut tx, &run, checkpoint.as_ref()).await?;
        let set_now = risks_set_now(&mut tx).await?;
        cancel_lost_validations(&mut tx, &run).await?;
        tx.commit().await?;

        let scope = RunScope {
            tenant_id,
            run_id,
            user_id: run.on_behalf_of_user_id,
            conversation_id: run.conversation_id,
        };
        let unknown = of_unknown_effect(&started, approved, &set_now);
        let Some(state) = run.state.clone() else {
            self.end(scope, STOPPED_BEFORE_ITS_FIRST_STEP, None).await?;
            return Ok(true);
        };
        if Utc::now() - run.updated_at > MAX_INTERRUPTION {
            let error = if unknown.is_empty() {
                INTERRUPTED_TOO_LONG.to_owned()
            } else {
                ending(TOO_LONG, &unknown, approved)
            };
            self.end(scope, &error, checkpoint).await?;
            return Ok(true);
        }
        let Some(access) = self.access.active_context(tenant_id, scope.user_id).await? else {
            let error = if unknown.is_empty() {
                ACCOUNT_INACTIVE.to_owned()
            } else {
                ending(&format!("{ACCOUNT_INACTIVE}."), &unknown, approved)
            };
            self.end(scope, &error, checkpoint).await?;
            return Ok(true);
        };
        // Should the engine not even start, the actions to check are named.
        let crash_message = if unknown.is_empty() {
            CRASH_MESSAGE.to_owned()
        } else {
            ending(NOT_TAKEN_UP, &unknown, approved)
        };
        let mode = ExecutionMode::Recover {
            state,
            interrupted: started,
            crash_message,
        };
        self.launch(Arc::new(access), run, mode);
        Ok(true)
    }

    /// Ends a run that does not go on, as the engine would: its outcome saved
    /// (status, message in the conversation, audit), then announced.
    pub(super) async fn end(
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
