//! Execution leases (ADR-083): this process's executor, the runs it
//! executes and their leases. A run's lease is taken in the transaction that
//! sets it running, renewed while it executes here, and a run whose lease
//! expired is taken up by an executor that does not execute it, under the
//! rules of ADR-053.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mimorya_db::TenantTx;
use mimorya_db::repositories::run_leases::{expired_run_leases, renew_run_leases, take_run_lease};
use mimorya_db::repositories::runs::RunRow;
use uuid::Uuid;

use tokio::sync::watch;

use super::executor::ExecutionMode;
use super::recovery::TakeUp;
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

/// This process's executor, drawn at startup, and the runs it executes.
pub(super) struct Leases {
    owner: Uuid,
    timing: LeaseTiming,
    /// Run, with its company and how many executions of it are under way
    /// here: one paused may still be ending while a decision resumes it.
    executing: Mutex<HashMap<Uuid, (Uuid, usize)>>,
}

impl Leases {
    pub(super) fn new(timing: LeaseTiming) -> Self {
        Self {
            owner: Uuid::new_v4(),
            timing,
            executing: Mutex::new(HashMap::new()),
        }
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

    fn register(self: &Arc<Self>, tenant_id: Uuid, run_id: Uuid) -> Executing {
        self.executing
            .lock()
            .expect("executing runs")
            .entry(run_id)
            .or_insert((tenant_id, 0))
            .1 += 1;
        Executing {
            leases: self.clone(),
            run_id,
        }
    }

    /// The runs executing here, by company.
    fn executing_by_tenant(&self) -> HashMap<Uuid, Vec<Uuid>> {
        let mut by_tenant: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for (run, (tenant, _)) in self.executing.lock().expect("executing runs").iter() {
            by_tenant.entry(*tenant).or_default().push(*run);
        }
        by_tenant
    }
}

/// An execution of a run here, until dropped: it ended, however. The run
/// executes here as long as one of its executions does.
struct Executing {
    leases: Arc<Leases>,
    run_id: Uuid,
}

impl Drop for Executing {
    fn drop(&mut self) {
        let mut executing = self.leases.executing.lock().expect("executing runs");
        if let Some((_, count)) = executing.get_mut(&self.run_id) {
            *count -= 1;
            if *count == 0 {
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
        let executing = self.leases.register(access.subject.tenant_id, run.id);
        self.tasks.spawn(async move {
            executor.execute(access, run, mode).await;
            drop(executing);
        });
    }

    /// Renews the leases of the runs executing here, company after company,
    /// one failing leaving the others to renew; how many it still held.
    pub async fn renew_leases(&self) -> u64 {
        let mut renewed = 0;
        for (tenant_id, runs) in self.leases.executing_by_tenant() {
            match self.renew_in(tenant_id, &runs).await {
                Ok(count) => renewed += count,
                Err(error) => {
                    tracing::warn!(tenant_id = %tenant_id, error = format!("{error:#}"), "Run leases not renewed");
                }
            }
        }
        renewed
    }

    async fn renew_in(&self, tenant_id: Uuid, runs: &[Uuid]) -> anyhow::Result<u64> {
        let mut tx = self.db.begin(tenant_id).await?;
        let ttl = self.leases.ttl_seconds();
        let renewed = renew_run_leases(&mut tx, self.leases.owner(), runs, ttl).await?;
        tx.commit().await?;
        Ok(renewed)
    }

    /// Takes up the runs whose lease expired and which do not execute here,
    /// as after a stop of the service (ADR-053): resumed, or ended naming
    /// the actions to check; how many. Only in the process that executes
    /// runs, as expiries (ADR-061): another leaves them to it. Stops between
    /// two runs once the service stops (`stopping`).
    pub async fn take_up_expired(&self, stopping: &watch::Sender<bool>) -> anyhow::Result<usize> {
        if !self.executes().await {
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
            match self.recover(run, TakeUp::Expired).await {
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
        let ending = leases.register(tenant, run);
        let resumed = leases.register(tenant, run);
        drop(ending);
        assert!(leases.executes(run));
        assert_eq!(leases.executing_by_tenant()[&tenant], [run]);
        drop(resumed);
        assert!(!leases.executes(run));
        assert!(leases.executing_by_tenant().is_empty());
    }
}
