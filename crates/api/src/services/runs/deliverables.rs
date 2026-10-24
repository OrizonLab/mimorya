//! Deliverables produced by tools during a run (draft plannings…), stored in
//! PostgreSQL and owned by the user the agent acts for; written under the
//! execution's lease (ADR-086).

use async_trait::async_trait;
use mimorya_core::tools::{Deliverable, DeliverableInput, DeliverableService, DeliverableStatus};
use mimorya_db::repositories::deliverables::{
    DeliverableRow, NewDeliverable, create_deliverable, get_deliverable, set_deliverable_status,
};
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use super::db_ports::RunScope;
use super::fence::LeaseFence;

pub struct DbDeliverables {
    db: Database,
    scope: RunScope,
    /// Held by each change (ADR-086).
    fence: LeaseFence,
}

impl DbDeliverables {
    pub fn new(db: Database, scope: RunScope, fence: LeaseFence) -> Self {
        Self { db, scope, fence }
    }

    async fn held_tx(&self) -> anyhow::Result<TenantTx> {
        let mut tx = self.db.begin(self.scope.tenant_id).await?;
        self.fence.hold_in(&mut tx, self.scope.run_id).await?;
        Ok(tx)
    }
}

fn to_deliverable(row: DeliverableRow) -> anyhow::Result<Deliverable> {
    let status = DeliverableStatus::parse(&row.status)
        .ok_or_else(|| anyhow::anyhow!("unknown deliverable status {}", row.status))?;
    Ok(Deliverable {
        id: row.id,
        kind: row.kind,
        title: row.title,
        content: row.content,
        org_unit_id: row.org_unit_id,
        owner_user_id: row.owner_user_id,
        run_id: row.run_id,
        status,
    })
}

#[async_trait]
impl DeliverableService for DbDeliverables {
    async fn create(&self, input: DeliverableInput) -> anyhow::Result<Deliverable> {
        let mut tx = self.held_tx().await?;
        let row = create_deliverable(
            &mut tx,
            NewDeliverable {
                tenant_id: self.scope.tenant_id,
                run_id: Some(self.scope.run_id),
                owner_user_id: self.scope.user_id,
                org_unit_id: input.org_unit_id,
                kind: input.kind,
                title: input.title,
                content: input.content,
            },
        )
        .await?;
        tx.commit().await?;
        to_deliverable(row)
    }

    async fn get(&self, id: Uuid) -> anyhow::Result<Option<Deliverable>> {
        let mut tx = self.db.begin(self.scope.tenant_id).await?;
        let row = get_deliverable(&mut tx, id).await?;
        tx.commit().await?;
        row.map(to_deliverable).transpose()
    }

    async fn set_status(&self, id: Uuid, status: DeliverableStatus) -> anyhow::Result<()> {
        let mut tx = self.held_tx().await?;
        set_deliverable_status(&mut tx, id, status.as_str()).await?;
        Ok(tx.commit().await?)
    }
}
