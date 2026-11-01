//! The passes that play the evaluations (ADR-104, ADR-157), in a process
//! that executes runs: one per company, each holding its company's lock
//! (ADR-086) for as long as it plays, so that no other process plays that
//! company's evaluations meanwhile, and one company's never holds up
//! another's. A process plays a few companies at once, the one whose
//! evaluation waits longest first; the others wait for its next pass.
//! Under its company's lock, a pass fails an evaluation found running: a
//! stopped process left it, to be asked for again. It plays the company's
//! queued ones oldest first, until none is left, or until it lost its
//! lock's session (PostgreSQL restarted): found before each evaluation, or
//! once another pass, having taken the lock, ended the one it played. It
//! stops there, lest it end what that one plays. The passes are started
//! every minute, and as soon as an evaluation is asked for in a process
//! that executes runs.

use std::sync::Arc;

use mimorya_db::SweepLock;
use mimorya_db::repositories::skill_evaluations::{PendingEvaluation, pending_evaluations};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use uuid::Uuid;

use super::play::{fail_interrupted, play};
use crate::app::{AppState, Services};

/// Companies whose evaluations one process plays at once.
pub const COMPANIES_AT_ONCE: usize = 4;

/// Evaluations one pass plays at most for a company: the next one goes on.
const MAX_PER_PASS: usize = 50;

/// The places of the companies one process plays at once.
#[derive(Clone)]
pub struct EvaluationSlots(Arc<Semaphore>);

impl Default for EvaluationSlots {
    fn default() -> Self {
        Self(Arc::new(Semaphore::new(COMPANIES_AT_ONCE)))
    }
}

impl EvaluationSlots {
    /// A place for one company's pass, held until dropped; `None` when this
    /// process plays as many as it may.
    pub fn take(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.0).try_acquire_owned().ok()
    }
}

/// The lock of the company `tenant_id`'s pass (ADR-086), whose connection
/// is named after it within PostgreSQL's 63 characters.
pub fn evaluation_sweep(tenant_id: Uuid) -> String {
    format!("evaluations:{tenant_id}")
}

/// Plays the evaluations waiting, in the companies no other pass plays,
/// unless this process executes no runs; how many it played.
pub async fn play_pending_evaluations(services: &AppState) -> anyhow::Result<usize> {
    if !services.runs.executes() {
        return Ok(0);
    }
    let pending = pending_evaluations(services.db.pool()).await?;
    let mut passes = JoinSet::new();
    for tenant_id in companies(&pending) {
        let Some(slot) = services.evaluation_slots.take() else {
            break;
        };
        let name = evaluation_sweep(tenant_id);
        let lock = match services.db.try_sweep_lock(&name).await {
            Ok(Some(lock)) => lock,
            Ok(None) => continue,
            Err(error) => {
                // Said now, not once the passes started end: the next pass
                // tries again.
                tracing::warn!(
                    tenant_id = %tenant_id,
                    error = %error,
                    "Skill evaluation lock not taken"
                );
                break;
            }
        };
        let services = Arc::clone(services);
        passes.spawn(async move {
            let played = play_company(&services, tenant_id, lock).await;
            drop(slot);
            played
        });
    }
    let mut played = 0;
    while let Some(joined) = passes.join_next().await {
        match joined {
            Ok(company) => played += company,
            Err(error) => tracing::warn!(error = %error, "Skill evaluation pass stopped"),
        }
    }
    Ok(played)
}

/// The companies with an evaluation to play or to fail, the one whose
/// evaluation waits longest first.
fn companies(pending: &[PendingEvaluation]) -> Vec<Uuid> {
    let mut companies: Vec<Uuid> = Vec::new();
    for evaluation in pending {
        if !companies.contains(&evaluation.tenant_id) {
            companies.push(evaluation.tenant_id);
        }
    }
    companies
}

/// Plays the company's evaluations under its lock, which the caller took
/// (`evaluation_sweep(tenant_id)`: no other lock keeps another pass out),
/// then releases it; how many it played, a failure ending its pass alone.
pub async fn play_company(services: &Services, tenant_id: Uuid, mut lock: SweepLock) -> usize {
    let mut played = 0;
    if let Err(error) = play_under_lock(services, tenant_id, &mut lock, &mut played).await {
        tracing::warn!(
            tenant_id = %tenant_id,
            error = format!("{error:#}"),
            "Skill evaluation pass failed"
        );
    }
    lock.release().await;
    played
}

async fn play_under_lock(
    services: &Services,
    tenant_id: Uuid,
    lock: &mut SweepLock,
    played: &mut usize,
) -> anyhow::Result<()> {
    while *played < MAX_PER_PASS && lock.held().await {
        let pending = pending_evaluations(services.db.pool()).await?;
        let (left, queued): (Vec<&PendingEvaluation>, Vec<_>) = pending
            .iter()
            .filter(|p| p.tenant_id == tenant_id)
            .partition(|p| p.running);
        for left in left {
            fail_interrupted(services, tenant_id, left.id).await?;
        }
        let Some(next) = queued.first() else {
            break;
        };
        let held = play(services, tenant_id, next.id).await?;
        *played += 1;
        if !held {
            break;
        }
    }
    Ok(())
}

/// Starts the passes in the background, without waiting for them.
pub fn wake(services: &AppState) {
    let services = services.clone();
    tokio::spawn(async move {
        match play_pending_evaluations(&services).await {
            Ok(0) => {}
            Ok(played) => tracing::info!(played, "Skill evaluations played"),
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "Skill evaluation passes failed"
                )
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(tenant_id: Uuid, running: bool) -> PendingEvaluation {
        PendingEvaluation {
            tenant_id,
            id: Uuid::new_v4(),
            running,
        }
    }

    #[test]
    fn each_company_comes_once_the_one_waiting_longest_first() {
        let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
        let listed = [
            pending(second, true),
            pending(first, false),
            pending(second, false),
            pending(first, false),
        ];
        assert_eq!(companies(&listed), [second, first]);
        assert!(companies(&[]).is_empty());
    }

    #[test]
    fn the_lock_s_connection_name_keeps_whole_within_postgresql_s_limit() {
        let name = format!("mimorya-sweep-{}", evaluation_sweep(Uuid::max()));
        assert!(name.len() <= 63, "{name}");
    }
}
