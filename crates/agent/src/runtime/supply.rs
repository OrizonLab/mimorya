//! The operator's supply of a run's model (ADR-208, step 1): before each
//! call to the model, the run asks whether a call of its role may still be
//! made on the operator's key. Past the company's monthly cap or prepaid
//! credit, or once the operator no longer supplies it, none is: the run
//! stops before its next call, its user told why; a classification is not
//! asked, an answer not judged. A call on the company's own key is never
//! held back. The stop is journaled once a run, without content.

use async_trait::async_trait;
use mimorya_core::model_supply::SupplyStop;
use serde_json::json;

use super::ports::EngineAuditEntry;
use super::state::RunCheckpoint;
use super::types::RuntimeDeps;
use crate::llm::ModelRole;

/// What the operator's supply of the company allows now.
#[async_trait]
pub trait SupplyLimit: Send + Sync {
    /// Why a call of `role` may not be made: `None` while it may, or when
    /// it goes to the company's own key.
    async fn stopped(&self, role: ModelRole) -> anyhow::Result<Option<SupplyStop>>;
}

/// Before a call of `role`: why it may not be made, if so, journaled once a
/// run.
pub(super) async fn supply_stopped(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
    role: ModelRole,
) -> anyhow::Result<Option<SupplyStop>> {
    let Some(limit) = &deps.supply else {
        return Ok(None);
    };
    let Some(stop) = limit.stopped(role).await? else {
        return Ok(None);
    };
    if !state.supply_stopped {
        state.supply_stopped = true;
        deps.ports
            .audit
            .record(EngineAuditEntry {
                action: "model_supply.limited".into(),
                outcome: "denied",
                resource_type: None,
                resource_id: None,
                reason: Some(stop.reason().into()),
                metadata: json!({}),
            })
            .await?;
    }
    Ok(Some(stop))
}
