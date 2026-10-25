//! The pass that plays the evaluations (ADR-104), in a process that executes
//! runs, one at a time across the processes: it holds the lock of its sweep
//! (ADR-086) for as long as it plays. Under that lock no other process
//! plays one, so an evaluation found running was left by a stopped process:
//! it fails, to be asked for again. The queued ones are played oldest
//! first, until none is left, or until this pass lost the lock's session
//! (PostgreSQL restarted): found before each evaluation, or once another
//! pass, having taken the lock, ended the one this pass played. It stops
//! there, lest it end what that one plays. The pass is started every
//! minute, and as soon as an evaluation is asked for in a process that
//! executes runs.

use mimorya_db::SweepLock;
use mimorya_db::repositories::skill_evaluations::pending_evaluations;

use super::play::{fail_interrupted, play};
use crate::app::{AppState, Services};

/// The lock of the pass (ADR-086).
pub const EVALUATION_SWEEP: &str = "skill-evaluations";

/// Evaluations played by one pass at most: the next one goes on.
const MAX_PER_PASS: usize = 50;

/// Plays the evaluations waiting, unless this process executes no runs or
/// another plays them; how many it played.
pub async fn play_pending_evaluations(services: &Services) -> anyhow::Result<usize> {
    if !services.runs.executes() {
        return Ok(0);
    }
    let Some(mut lock) = services.db.try_sweep_lock(EVALUATION_SWEEP).await? else {
        return Ok(0);
    };
    let played = play_under_lock(services, &mut lock).await;
    lock.release().await;
    played
}

async fn play_under_lock(services: &Services, lock: &mut SweepLock) -> anyhow::Result<usize> {
    let mut played = 0;
    while played < MAX_PER_PASS && lock.held().await {
        let pending = pending_evaluations(services.db.pool()).await?;
        for left in pending.iter().filter(|p| p.running) {
            fail_interrupted(services, left.tenant_id, left.id).await?;
        }
        let Some(next) = pending.iter().find(|p| !p.running) else {
            break;
        };
        let held = play(services, next.tenant_id, next.id).await?;
        played += 1;
        if !held {
            break;
        }
    }
    Ok(played)
}

/// Starts a pass in the background, without waiting for it.
pub fn wake(services: &AppState) {
    let services = services.clone();
    tokio::spawn(async move {
        match play_pending_evaluations(&services).await {
            Ok(0) => {}
            Ok(played) => tracing::info!(played, "Skill evaluations played"),
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "Skill evaluation pass failed")
            }
        }
    });
}
