//! Runs whose executor stopped (ADR-053, ADR-086): their lease expired, or
//! they never had one. A process that executes runs takes them up at
//! startup, then whenever a lease expires (`leases.rs`), side by side with
//! the other workers: whichever locks a run first takes it up, the others
//! find it taken. Each run goes on in its own company, from its last saved
//! step, through its executor; the engine decides whether going on is safe.
//! The validation requests of the lost step are cancelled first, and audited
//! (the run asks again if it needs to). A run stopped before its first saved
//! step, for too long, or whose user may no longer act, ends there, naming
//! the actions that were under way when it stopped. Its taking up counts as
//! progress against the sweep of stuck runs; how long it may stay
//! interrupted still counts from its last step.

mod calls;
mod executing;

use std::sync::Arc;

use chrono::Utc;
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, RunCheckpoint, RunEvent, RunOutcome};
use mimorya_db::repositories::run_leases::{mark_taken_up, run_lease_expired};
use mimorya_db::repositories::runs::{InterruptedRun, RunStatus, lock_run};
use tokio::sync::watch;

use super::db_ports::RunScope;
use super::events::record_run_event;
use super::executor::ExecutionMode;
use super::outcome::{CRASH_MESSAGE, persist_outcome};
use super::service::RunService;
use calls::{
    cancel_lost_validations, ending, of_unknown_effect, risks_set_now, started_since_saved,
};
pub use executing::NOT_AN_EXECUTOR;

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

impl RunService {
    /// Starts executing runs, then takes up those whose executor stopped,
    /// until the service stops (`stopping`); how many it took up. Called
    /// once, at startup, once the API listens; refused in an `api` process.
    pub async fn recover_interrupted(
        &self,
        stopping: &watch::Sender<bool>,
    ) -> anyhow::Result<usize> {
        self.start_executing()?;
        let taken_up = self.take_up_expired(stopping).await;
        // Over, whatever came of it: what waited for it may go on.
        self.taken_up.send_replace(true);
        taken_up
    }

    /// Resolves once this process has taken up, at startup, the runs whose
    /// executor stopped: its queue, its takeover of expired leases and its
    /// scheduled tasks then go on.
    pub async fn interrupted_runs_taken_up(&self) {
        let mut taken_up = self.taken_up.subscribe();
        // The sender lives as long as `self`: this ends only once taken up.
        let _ = taken_up.wait_for(|done| *done).await;
    }

    /// Takes the run up, under its lock and lease: resumed, or ended naming
    /// the actions to check; whether it was taken (it may have ended, or its
    /// lease been renewed, meanwhile).
    pub(super) async fn recover(&self, interrupted: InterruptedRun) -> anyhow::Result<bool> {
        let InterruptedRun { tenant_id, run_id } = interrupted;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(run) = lock_run(&mut tx, run_id).await? else {
            return Ok(false);
        };
        if run.status() != RunStatus::Running
            || self.leases.executes(run_id)
            || !run_lease_expired(&mut tx, run_id).await?
        {
            return Ok(false);
        }
        self.hold(&mut tx, run_id).await?;
        mark_taken_up(&mut tx, run_id).await?;
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
        persist_outcome(self.db.begin(scope.tenant_id).await?, scope, &outcome).await?;
        let finished = RunEvent::RunFinished {
            status: outcome.status().into(),
            error: Some(error.into()),
        };
        let bus = &self.executor.bus;
        record_run_event(&self.db, bus, scope.tenant_id, scope.run_id, finished).await?;
        Ok(())
    }
}
