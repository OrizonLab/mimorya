//! Where a run opens its sub-agents' runs (ADR-098): each recorded beside
//! it, under its lease, for the same company and user, and given its own
//! journal. A sub-agent's run opens none.

use async_trait::async_trait;
use mimorya_agent::runtime::{SubRun, SubRunRequest, SubRunStore};
use mimorya_db::repositories::sub_runs::{NewSubRun, create_sub_run};

use super::db_ports::DbPorts;

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
