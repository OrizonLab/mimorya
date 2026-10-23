//! Execution leases (ADR-083): this process's executor, the runs it
//! executes and their leases. A run's lease is taken in the transaction that
//! sets it running, renewed while it executes here, and a run whose lease
//! expired is taken up by an executor that does not execute it, under the
//! rules of ADR-053.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use mimorya_db::TenantTx;
use mimorya_db::repositories::run_leases::{expired_run_leases, renew_run_leases, take_run_lease};
use mimorya_db::repositories::runs::RunRow;
use uuid::Uuid;

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
    /// Run, and its company, while it executes here.
    executing: Mutex<HashMap<Uuid, Uuid>>,
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
            .insert(run_id, tenant_id);
        Executing {
            leases: self.clone(),
            run_id,
        }
    }

    /// The runs executing here, by company.
    fn executing_by_tenant(&self) -> HashMap<Uuid, Vec<Uuid>> {
        let mut by_tenant: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for (run, tenant) in self.executing.lock().expect("executing runs").iter() {
            by_tenant.entry(*tenant).or_default().push(*run);
        }
        by_tenant
    }
}

/// A run executing here, until dropped: its execution ended, however.
struct Executing {
    leases: Arc<Leases>,
    run_id: Uuid,
}

impl Drop for Executing {
    fn drop(&mut self) {
        self.leases
            .executing
            .lock()
            .expect("executing runs")
            .remove(&self.run_id);
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

    /// Renews the leases of the runs executing here; how many it still held.
    pub async fn renew_leases(&self) -> anyhow::Result<u64> {
        let mut renewed = 0;
        for (tenant_id, runs) in self.leases.executing_by_tenant() {
            let mut tx = self.db.begin(tenant_id).await?;
            let ttl = self.leases.ttl_seconds();
            renewed += renew_run_leases(&mut tx, self.leases.owner(), &runs, ttl).await?;
            tx.commit().await?;
        }
        Ok(renewed)
    }

    /// Takes up the runs whose lease expired by `now` and which do not
    /// execute here, as after a stop of the service (ADR-053): resumed, or
    /// ended naming the actions to check; how many.
    pub async fn take_up_expired(&self, now: DateTime<Utc>) -> anyhow::Result<usize> {
        let mut taken = 0;
        for run in expired_run_leases(self.db.pool(), now).await? {
            // Checked again under the run's lock, with whether it executes
            // here. One that cannot be taken up stays running: the sweep of
            // stuck runs fails it later.
            match self.recover(run, TakeUp::Expired(now)).await {
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
