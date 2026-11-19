//! The operator's supply of the company's model as those who manage its
//! settings read it (`settings:manage`, company-wide, ADR-210): its formula,
//! its cap or its credit, what the month was re-invoiced, the credit's
//! payments and whether the supply holds its calls back now. Read as the
//! application, under its company's row-level security; the operator alone
//! writes it (ADR-208).

use chrono::{DateTime, Utc};
use mimorya_core::model_supply::{SupplyState, SupplyStop, supply_stop};
use mimorya_core::time_zone::month_start;
use mimorya_db::Database;
use mimorya_db::repositories::model_supply::{CreditEntryRow, credit_entries, supply_state};

use crate::services::{AccessContext, LlmService};

const PERMISSION: &str = "settings:manage";

/// What an administrator reads of the company's supply.
pub struct SupplyOverview {
    pub state: SupplyState,
    pub month_start: DateTime<Utc>,
    /// The credit's payments, the earliest first.
    pub payments: Vec<CreditEntryRow>,
    pub runs_on_operator_key: bool,
    /// Why no call goes to the operator's key now, under its formula.
    pub stopped: Option<SupplyStop>,
}

pub async fn company_model_supply(
    db: &Database,
    llm: &LlmService,
    access: &AccessContext,
) -> anyhow::Result<SupplyOverview> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let month_start = month_start(Utc::now());
    let mut tx = db.begin(tenant_id).await?;
    let state = supply_state(&mut tx, tenant_id, month_start).await?;
    let payments = credit_entries(&mut tx, tenant_id).await?;
    tx.commit().await?;
    let runs_on_operator_key = llm.resolve(tenant_id).await?.runs_on_operator_key();
    // Without a formula of the operator's, nothing of it is held back.
    let stopped = state
        .formula
        .on_operator_key()
        .then(|| supply_stop(&state))
        .flatten();
    Ok(SupplyOverview {
        state,
        month_start,
        payments,
        runs_on_operator_key,
        stopped,
    })
}
