//! The operator's supply of the company's model as those who manage its
//! settings read it (`settings:manage`, company-wide, ADR-210): its formula,
//! its cap or its credit, what the month was re-invoiced, the credit's
//! balance and payments, where its runs go and whether the supply holds them
//! back now. Read as the application, under its company's row-level
//! security; the operator alone writes it (ADR-208).

use chrono::{DateTime, Utc};
use mimorya_core::model_supply::{SupplyFormula, SupplyState, SupplyStop, supply_stop};
use mimorya_core::time_zone::month_start;
use mimorya_db::Database;
use mimorya_db::repositories::model_supply::{CreditEntryRow, credit_entries, supply_state};

use crate::services::llm::Resolution;
use crate::services::{AccessContext, LlmService};

const PERMISSION: &str = "settings:manage";

/// Where the company's runs go now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunsOn {
    OperatorKey,
    OwnProvider,
    Nothing,
}

/// What an administrator reads of the company's supply.
pub struct SupplyOverview {
    pub state: SupplyState,
    pub month_start: DateTime<Utc>,
    /// The credit's balance, once the company has a credit (prepaid now, or
    /// a payment ever recorded).
    pub balance_micros: Option<i64>,
    /// The credit's payments, the latest first.
    pub payments: Vec<CreditEntryRow>,
    pub runs: RunsOn,
    /// Why its runs go no further on the operator's key, if they go there.
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
    let mut payments = credit_entries(&mut tx, tenant_id).await?;
    tx.commit().await?;
    payments.reverse();
    let runs = match llm.resolve(tenant_id).await?.runs {
        Resolution::Platform(Some(_)) => RunsOn::OperatorKey,
        Resolution::Platform(None) => RunsOn::Nothing,
        Resolution::Company(_) | Resolution::Refused(_) => RunsOn::OwnProvider,
    };
    let credited = state.formula == SupplyFormula::Prepaid || !payments.is_empty();
    // Nothing of the operator's is held back from runs that do not use it.
    let stopped = match runs {
        RunsOn::OperatorKey => supply_stop(&state),
        RunsOn::OwnProvider | RunsOn::Nothing => None,
    };
    Ok(SupplyOverview {
        balance_micros: credited.then_some(state.prepaid_balance_micros),
        state,
        month_start,
        payments,
        runs,
        stopped,
    })
}
