//! Execution leases (ADR-083): this process's executor, the runs it
//! executes and their leases. A run's lease is taken in the transaction that
//! sets it running, renewed while it executes here; an execution whose
//! lease a renewal finds gone, or that went unrenewed for too long, stops
//! (`fence.rs`, ADR-086). A run whose lease
//! expired, or which never had one, is taken up by an executor that does
//! not execute it, under the rules of ADR-053, whichever of the workers
//! locks it first (ADR-086).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mimorya_db::TenantTx;
use mimorya_db::repositories::run_leases::{expired_run_leases, renew_run_leases, take_run_lease};
use mimorya_db::repositories::runs::RunRow;
use uuid::Uuid;

use tokio::sync::watch;

use super::executor::ExecutionMode;
use super::fence::LeaseFence;
use super::service::RunService;
use crate::services::access::AccessContext;

/// How long a lease lasts, how often the runs executing here renew theirs,
/// and how often expired ones are taken up.
#[derive(Debug, Clone, Copy)]
pub struct LeaseTiming {
    pub ttl: Duration,
    pub renew_every: Duration,
    pub take_up_every: Duration,
}

impl Default for LeaseTiming {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(60),
            renew_every: Duration::from_secs(20),
            take_up_every: Duration::from_secs(60),
        }
    }
}

/// The executions of a run under way here, with its company: one paused
/// may still be ending while a decision resumes it.
struct Executions {
    tenant_id: Uuid,
    fences: Vec<(u64, LeaseFence)>,
}

/// This process's executor, drawn at startup, and the runs it executes.
pub(super) struct Leases {
    owner: Uuid,
    timing: LeaseTiming,
    executing: Mutex<HashMap<Uuid, Executions>>,
    next_execution: AtomicU64,
}

impl Leases {
    pub(super) fn new(timing: LeaseTiming) -> Self {
        Self {
            owner: Uuid::new_v4(),
            timing,
            executing: Mutex::new(HashMap::new()),
            next_execution: AtomicU64::new(0),
        }
    }

    /// How long a lease counts as held here without a renewal: less than
    /// its life, by a renewal's interval, before anyone may take it up.
    fn holds_for(&self) -> Duration {
        let LeaseTiming {
            ttl, renew_every, ..
        } = self.timing;
        ttl.checked_sub(renew_every).unwrap_or(ttl / 2)
    }

    pub(super) fn owner(&self) -> Uuid {
        self.owner
    }

    pub(super) fn ttl_seconds(&self) -> f64 {
        self.timing.ttl.as_secs_f64()
    }

    /// Whether the run executes here, whatever its lease says.
    pub(super) fn executes(&self, run_id: Uuid) -> bool {
        self.executing
            .lock()
            .expect("executing runs")
            .contains_key(&run_id)
    }

    /// A new execution of the run, whose lease this executor just took,
    /// and its fence.
    fn register(self: &Arc<Self>, tenant_id: Uuid, run_id: Uuid) -> (Executing, LeaseFence) {
        let id = self.next_execution.fetch_add(1, Ordering::Relaxed);
        let fence = LeaseFence::new(self.owner, self.holds_for());
        self.executing
            .lock()
            .expect("executing runs")
            .entry(run_id)
            .or_insert_with(|| Executions {
                tenant_id,
                fences: Vec::new(),
            })
            .fences
            .push((id, fence.clone()));
        let executing = Executing {
            leases: self.clone(),
            run_id,
            id,
        };
        (executing, fence)
    }

    /// The runs executing here, by company, with their executions' fences.
    fn executing_by_tenant(&self) -> HashMap<Uuid, Vec<(Uuid, Vec<LeaseFence>)>> {
        let mut by_tenant: HashMap<Uuid, Vec<(Uuid, Vec<LeaseFence>)>> = HashMap::new();
        for (run, executions) in self.executing.lock().expect("executing runs").iter() {
            let fences = executions.fences.iter().map(|(_, f)| f.clone()).collect();
            by_tenant
                .entry(executions.tenant_id)
                .or_default()
                .push((*run, fences));
        }
        by_tenant
    }
}

/// An execution of a run here, until dropped: it ended, however. The run
/// executes here as long as one of its executions does.
struct Executing {
    leases: Arc<Leases>,
    run_id: Uuid,
    id: u64,
}

impl Drop for Executing {
    fn drop(&mut self) {
        let mut executing = self.leases.executing.lock().expect("executing runs");
        if let Some(executions) = executing.get_mut(&self.run_id) {
            executions.fences.retain(|(id, _)| *id != self.id);
            if executions.fences.is_empty() {
                executing.remove(&self.run_id);
            }
        }
    }
}

impl RunService {
    /// This process's executor, which holds the leases of the runs it
    /// executes.
    pub fn executor_id(&self) -> Uuid {
        self.leases.owner()
    }

    pub fn lease_timing(&self) -> LeaseTiming {
        self.leases.timing
    }

    /// Takes `run_id`'s lease for this executor, in the transaction that
    /// sets the run running: whoever sets it going executes it.
    pub(super) async fn hold(&self, tx: &mut TenantTx, run_id: Uuid) -> anyhow::Result<()> {
        let ttl = self.leases.ttl_seconds();
        take_run_lease(tx, run_id, self.leases.owner(), ttl).await?;
        Ok(())
    }

    /// Executes `run`, whose lease this executor holds, in the background;
    /// its lease is renewed while it executes here.
    pub(super) fn launch(&self, access: Arc<AccessContext>, run: RunRow, mode: ExecutionMode) {
        let executor = self.executor.clone();
        // Registered from now: never taken for one nobody executes.
        let (executing, fence) = self.leases.register(access.subject.tenant_id, run.id);
        self.tasks.spawn(async move {
            executor.execute(access, run, mode, fence).await;
            drop(executing);
        });
    }

    /// Renews the leases of the runs executing here, company after company,
    /// one failing leaving the others to renew; how many it still held.
    /// The executions of a run whose lease is no longer held here stop
    /// before their next step (ADR-086).
    pub async fn renew_leases(&self) -> u64 {
        let mut renewed = 0;
        for (tenant_id, executing) in self.leases.executing_by_tenant() {
            let runs: Vec<Uuid> = executing.iter().map(|(run, _)| *run).collect();
            match self.renew_in(tenant_id, &runs).await {
                Ok(held) => {
                    for (run, fences) in &executing {
                        let still = held.contains(run);
                        for fence in fences {
                            if still { fence.renewed() } else { fence.lose() }
                        }
                    }
                    renewed += held.len() as u64;
                }
                Err(error) => {
                    tracing::warn!(tenant_id = %tenant_id, error = format!("{error:#}"), "Run leases not renewed");
                }
            }
        }
        renewed
    }

    async fn renew_in(&self, tenant_id: Uuid, runs: &[Uuid]) -> anyhow::Result<Vec<Uuid>> {
        let mut tx = self.db.begin(tenant_id).await?;
        let ttl = self.leases.ttl_seconds();
        let renewed = renew_run_leases(&mut tx, self.leases.owner(), runs, ttl).await?;
        tx.commit().await?;
        Ok(renewed)
    }

    /// Takes up the runs whose lease expired, or which never had one, and
    /// which do not execute here (ADR-053, ADR-086): resumed, or ended
    /// naming the actions to check; how many. Only in a process that
    /// executes runs, side by side with the others: each run is taken up
    /// by whichever locks it first. Stops between two runs once the service
    /// stops (`stopping`).
    pub async fn take_up_expired(&self, stopping: &watch::Sender<bool>) -> anyhow::Result<usize> {
        if !self.executes() {
            return Ok(0);
        }
        let mut taken = 0;
        for run in expired_run_leases(self.db.pool()).await? {
            if *stopping.borrow() {
                break;
            }
            // Checked again under the run's lock, with whether it executes
            // here. One that cannot be taken up stays running: the sweep of
            // stuck runs fails it later.
            match self.recover(run).await {
                Ok(true) => taken += 1,
                Ok(false) => {}
                Err(error) => {
                    tracing::error!(run_id = %run.run_id, error = format!("{error:#}"), "Run with an expired lease not taken up");
                }
            }
        }
        Ok(taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_executes_here_while_one_of_its_executions_does() {
        let leases = Arc::new(Leases::new(LeaseTiming::default()));
        let (tenant, run) = (Uuid::new_v4(), Uuid::new_v4());
        // Paused, still ending, when a decision resumes it.
        let (ending, _) = leases.register(tenant, run);
        let (resumed, fence) = leases.register(tenant, run);
        drop(ending);
        assert!(leases.executes(run));
        let executing = &leases.executing_by_tenant()[&tenant];
        assert_eq!(executing.len(), 1);
        assert_eq!(executing[0].0, run);
        // The execution left is the resumed one, its own fence.
        executing[0].1[0].lose();
        assert!(!fence.holds());
        drop(resumed);
        assert!(!leases.executes(run));
        assert!(leases.executing_by_tenant().is_empty());
    }
}
