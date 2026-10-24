//! An execution's hold on its run (ADR-086). Every transaction in which an
//! execution changes its run (journal, saved step, requests, outcome,
//! deliverables, memory) first locks the run while it still carries this
//! executor's lease: should another executor have taken the lease up, it
//! writes nothing. Before each step, the execution checks that its lease
//! was neither found lost by a renewal nor left unrenewed for longer than
//! another executor waits to take it up: past that, it calls no model and
//! no tool. Either way it stops, without recording anything: its run is
//! the other executor's.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mimorya_db::TenantTx;
use mimorya_db::repositories::run_leases::lock_held_lease;
use uuid::Uuid;

/// Why an execution stops: its run's lease went elsewhere, or may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseLost;

impl std::fmt::Display for LeaseLost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the run's lease is no longer this executor's: the execution stops")
    }
}

impl std::error::Error for LeaseLost {}

/// Whether `error` comes from a lost lease, whatever wraps it.
pub fn lost(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<LeaseLost>())
}

struct State {
    lost: bool,
    renewed: Instant,
}

/// One execution's lease on its run, as this process knows it.
#[derive(Clone)]
pub struct LeaseFence {
    owner: Uuid,
    /// How long after its last renewal the lease still counts as held.
    holds_for: Duration,
    state: Arc<Mutex<State>>,
}

impl LeaseFence {
    /// A lease of `owner`'s, just taken or renewed, that holds for
    /// `holds_for` without renewal.
    pub(super) fn new(owner: Uuid, holds_for: Duration) -> Self {
        Self {
            owner,
            holds_for,
            state: Arc::new(Mutex::new(State {
                lost: false,
                renewed: Instant::now(),
            })),
        }
    }

    /// Whether the execution may still act: its lease neither found lost
    /// nor unrenewed for too long.
    pub fn holds(&self) -> bool {
        let state = self.state.lock().expect("lease fence");
        !state.lost && state.renewed.elapsed() < self.holds_for
    }

    /// Fails once the execution may no longer act.
    pub fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.holds(), LeaseLost);
        Ok(())
    }

    /// Locks `run_id` in `tx` while it still carries this lease, which
    /// nobody can then take up before `tx` ends; fails otherwise.
    pub async fn hold_in(&self, tx: &mut TenantTx, run_id: Uuid) -> anyhow::Result<()> {
        self.check()?;
        if !lock_held_lease(tx, run_id, self.owner).await? {
            self.lose();
            anyhow::bail!(LeaseLost);
        }
        Ok(())
    }

    pub(super) fn owner(&self) -> Uuid {
        self.owner
    }

    pub(super) fn renewed(&self) {
        self.state.lock().expect("lease fence").renewed = Instant::now();
    }

    pub(super) fn lose(&self) {
        self.state.lock().expect("lease fence").lost = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lease_holds_until_found_lost_or_left_unrenewed() {
        let fence = LeaseFence::new(Uuid::new_v4(), Duration::from_secs(40));
        assert!(fence.holds());
        let copy = fence.clone();
        copy.lose();
        assert!(!fence.holds());
        assert!(lost(&fence.check().unwrap_err().context("checkpoint")));

        let unrenewed = LeaseFence::new(Uuid::new_v4(), Duration::ZERO);
        assert!(!unrenewed.holds());
        let renewed = LeaseFence::new(Uuid::new_v4(), Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(60));
        assert!(!renewed.holds());
        renewed.renewed();
        assert!(renewed.holds());
    }
}
