//! The operator's supply of the demo company's model (ADR-208): its
//! formula changed as by `mimorya-admin`, and what it spent on the
//! operator's key or has left of its credit set outright.

use mimorya_api::services::provisioning::{
    CapChoice, SupplyChange, change_model_supply, model_supply_report,
};
use mimorya_core::demo::tenant;
use mimorya_core::model_supply::SupplyFormula;
use sqlx::PgPool;

/// The smallest cap: 1 USD.
pub const CAP: i64 = 1_000_000;

pub async fn supply(owner: &PgPool, change: SupplyChange) {
    change_model_supply(owner, tenant::SLUG, change)
        .await
        .unwrap();
}

pub fn capped(cap: CapChoice) -> SupplyChange {
    SupplyChange::Formula {
        formula: SupplyFormula::Capped,
        cap,
    }
}

/// What the company spent on the operator's key: `billed_micros` this
/// month under `formula`, nothing else (what the binary's tests before
/// spent forgotten).
pub async fn spent(owner: &PgPool, formula: SupplyFormula, billed_micros: i64) {
    sqlx::query(
        "delete from usage_records where tenant_id = $1 and supply_formula <> 'client_key'",
    )
    .bind(tenant::ID)
    .execute(owner)
    .await
    .unwrap();
    if billed_micros == 0 {
        return;
    }
    sqlx::query(
        "insert into usage_records (tenant_id, kind, role, provider, model, cost_micros, \
         supply_formula, billed_micros) values ($1, 'llm', 'main', 'anthropic', \
         'claude-haiku-5-5', $2, $3, $2)",
    )
    .bind(tenant::ID)
    .bind(billed_micros)
    .bind(formula.as_str())
    .execute(owner)
    .await
    .unwrap();
}

/// Leaves the company's prepaid credit with `left`: what its payments came
/// to, less a consumption this month.
pub async fn credit_left(owner: &PgPool, left: i64) {
    let report = model_supply_report(owner, tenant::SLUG).await.unwrap();
    spent(
        owner,
        SupplyFormula::Prepaid,
        report.spending.credited_micros - left,
    )
    .await;
}
