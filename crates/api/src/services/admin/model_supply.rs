//! The operator's supply of the company's model as those who manage its
//! settings read it (`settings:manage`, company-wide, ADR-210): its formula,
//! its cap or its credit, what the month was re-invoiced, the credit's
//! balance and payments, where its runs go and whether the supply holds them
//! back now. Read as the application, under its company's row-level
//! security; the operator alone writes it (ADR-208).

use chrono::{DateTime, Utc};
use mimorya_core::model_supply::{
    DEFAULT_CAP_ALERTS, DEFAULT_CREDIT_ALERTS, SupplyFormula, SupplyState, SupplyStop, supply_stop,
};
use mimorya_core::time_zone::month_start;
use mimorya_db::Database;
use mimorya_db::repositories::model_supply::{
    CreditEntryRow, ModelSupplyRow, credit_entries, model_supply, supply_state,
};

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
    /// The shares of the cap and of the last payment its administrators
    /// are alerted at (ADR-211).
    pub cap_alerts: Vec<u8>,
    pub credit_alerts: Vec<u8>,
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
    let row = model_supply(&mut tx, tenant_id).await?;
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
        cap_alerts: row
            .as_ref()
            .map_or(DEFAULT_CAP_ALERTS.to_vec(), ModelSupplyRow::cap_alerts),
        credit_alerts: row.as_ref().map_or(
            DEFAULT_CREDIT_ALERTS.to_vec(),
            ModelSupplyRow::credit_alerts,
        ),
    })
}
