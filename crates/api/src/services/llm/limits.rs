//! The limits of the operator's supply (ADR-208, step 1), read before each
//! call on the operator's key: past the company's monthly cap or prepaid
//! credit, or once the operator no longer supplies it, no run is launched
//! and none goes on to its next call to the model. A call on the company's
//! own key is never held back. What the supply stands at is read anew at
//! each check, the company's models (whose key a call goes to) as they were
//! taken for the call. What is held back of a person's asking is traced by
//! its reason alone.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use mimorya_agent::llm::ModelRole;
use mimorya_agent::runtime::SupplyLimit;
use mimorya_core::model_supply::{SupplyFormula, SupplyStop, supply_stop};
use mimorya_core::time_zone::month_start;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::model_supply::supply_state;
use uuid::Uuid;

use super::Models;

/// Why a call of `role`, made with `models`, may not go to the operator's
/// key now; `None` while it may, or when it goes to the company's own.
pub async fn supply_stopped(
    db: &Database,
    models: &Models,
    tenant_id: Uuid,
    role: ModelRole,
) -> anyhow::Result<Option<SupplyStop>> {
    if models.charge(role, 0).formula == SupplyFormula::ClientKey {
        return Ok(None);
    }
    let mut tx = db.begin(tenant_id).await?;
    let state = supply_state(&mut tx, tenant_id, month_start(Utc::now())).await?;
    tx.commit().await?;
    Ok(supply_stop(&state))
}

/// Traces that the operator's supply held back what `user_id` asked for:
/// a run, an evaluation or a test of the connection, without content.
pub async fn trace_held_back(
    db: &Database,
    tenant_id: Uuid,
    user_id: Uuid,
    stop: SupplyStop,
) -> anyhow::Result<()> {
    let entry = AuditEntry::new(tenant_id, "user", "model_supply.limited", "denied")
        .by_user(user_id)
        .resource("tenant", tenant_id)
        .reason(stop.reason());
    let mut tx = db.begin(tenant_id).await?;
    append_audit(&mut tx, entry).await?;
    Ok(tx.commit().await?)
}

/// The limits a run's execution is held to, with the models it took.
pub struct SupplyGuard {
    db: Database,
    tenant_id: Uuid,
    models: Arc<Models>,
}

impl SupplyGuard {
    pub fn new(db: Database, tenant_id: Uuid, models: Arc<Models>) -> Self {
        Self {
            db,
            tenant_id,
            models,
        }
    }
}

#[async_trait]
impl SupplyLimit for SupplyGuard {
    async fn stopped(&self, role: ModelRole) -> anyhow::Result<Option<SupplyStop>> {
        supply_stopped(&self.db, &self.models, self.tenant_id, role).await
    }
}
