//! Where a run opens its sub-agents' runs (ADR-098): each recorded beside
//! it, under its lease, for the same company and user, and given its own
//! journal. A sub-agent's run opens none. Those still running when their
//! run is taken up, crashes or is cancelled end with it, their end
//! recorded in the same transaction.

use async_trait::async_trait;
use mimorya_agent::runtime::{RunEvent, SubRun, SubRunRequest, SubRunStore};
use mimorya_db::TenantTx;
use mimorya_db::repositories::runs::RunStatus;
use mimorya_db::repositories::sub_runs::{NewSubRun, create_sub_run, fail_running_sub_runs};
use uuid::Uuid;

use super::db_ports::DbPorts;
use super::events::append_in;
use crate::services::event_bus::{BusEvent, RunEventBus};

/// Why a sub-agent ends when its run crashes or is cancelled.
pub const SUB_AGENT_STOPPED: &str =
    "Sous-agent interrompu : la demande qui l’avait lancé s’est arrêtée.";

/// Fails `run_id`'s sub-agents still running, for `error`, their end
/// recorded in `tx`: the events to publish once it commits. Called before
/// the run itself ends, which would otherwise end them unannounced.
pub async fn end_sub_runs(
    tx: &mut TenantTx,
    bus: &RunEventBus,
    run_id: Uuid,
    error: &str,
) -> anyhow::Result<Vec<(Uuid, BusEvent)>> {
    let mut ended = Vec::new();
    for sub_run in fail_running_sub_runs(tx, run_id, error).await? {
        let finished = RunEvent::RunFinished {
            status: RunStatus::Failed.as_str().into(),
            error: Some(error.into()),
        };
        ended.push((sub_run, append_in(tx, bus, sub_run, finished).await?));
    }
    Ok(ended)
}

/// Publishes the ends `end_sub_runs` recorded, once committed.
pub fn announce(bus: &RunEventBus, ended: Vec<(Uuid, BusEvent)>) {
    for (sub_run, event) in ended {
        bus.publish(sub_run, event);
    }
}

#[async_trait]
impl SubRunStore for DbPorts {
    async fn open(&self, request: SubRunRequest) -> anyhow::Result<SubRun> {
        anyhow::ensure!(!self.sub_run, "a sub-agent's run opens no sub-agent");
        let mut tx = self.held_tx().await?;
        let row = create_sub_run(
            &mut tx,
            NewSubRun {
                parent_run_id: self.scope.run_id,
                profile: request.profile.as_str(),
                objective: request.objective,
                budget: serde_json::to_value(request.budget)?,
                allowed_permissions: request.allowed_permissions,
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("run {} not found", self.scope.run_id))?;
        tx.commit().await?;
        Ok(SubRun {
            run_id: row.id,
            ports: self.of_sub_run(row.id),
        })
    }
}
