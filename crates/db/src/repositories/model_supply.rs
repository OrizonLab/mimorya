//! How a company's model is supplied (ADR-208): its formula, cap and
//! markup, the prepaid credit's register, and what its calls on the
//! operator's key consumed. The application role reads them; the operator's
//! command writes them with the owner role, which row-level security does
//! not bound: every query names its company.

use chrono::{DateTime, Utc};
use mimorya_core::model_supply::{SupplyFormula, SupplyState};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ModelSupplyRow {
    /// `client_key`, `capped` or `prepaid`.
    pub formula: String,
    pub monthly_cap_micros: Option<i64>,
    pub unlimited: bool,
    pub markup_bps: i32,
    /// The shares of the cap and of the last payment its administrators
    /// are alerted at, in percent (ADR-211).
    pub cap_alert_percents: Vec<i16>,
    pub credit_alert_percents: Vec<i16>,
    pub updated_at: DateTime<Utc>,
}

impl ModelSupplyRow {
    /// The shares of the cap its administrators are alerted at.
    pub fn cap_alerts(&self) -> Vec<u8> {
        shares(&self.cap_alert_percents)
    }

    /// The shares of the last payment its administrators are alerted at.
    pub fn credit_alerts(&self) -> Vec<u8> {
        shares(&self.credit_alert_percents)
    }
}

/// Shares as stored, kept within 0 and 100 by the table.
fn shares(stored: &[i16]) -> Vec<u8> {
    stored
        .iter()
        .filter_map(|percent| u8::try_from(*percent).ok())
        .collect()
}

/// Locks the company for the operator's change of its supply, so that two
/// changes at once are traced each after the other; the references to the
/// company, which only share its key, are not held.
pub async fn lock_supply(conn: &mut PgConnection, tenant_id: Uuid) -> sqlx::Result<bool> {
    let locked: Option<i32> =
        sqlx::query_scalar("select 1 from tenants where id = $1 for no key update")
            .bind(tenant_id)
            .fetch_optional(conn)
            .await?;
    Ok(locked.is_some())
}

/// The company's formula, if the operator set one (else its own key).
pub async fn model_supply(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> sqlx::Result<Option<ModelSupplyRow>> {
    sqlx::query_as(
        "select formula, monthly_cap_micros, unlimited, markup_bps, cap_alert_percents, \
         credit_alert_percents, updated_at from tenant_model_supply where tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(conn)
    .await
}

/// A formula as the operator sets it: a cap or the unlimited mode for the
/// capped one alone.
#[derive(Debug, Clone, Copy)]
pub struct NewSupplyFormula {
    pub formula: SupplyFormula,
    pub monthly_cap_micros: Option<i64>,
    pub unlimited: bool,
}

/// Sets the company's formula, its markup kept (owner role).
pub async fn set_supply_formula(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    formula: NewSupplyFormula,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_model_supply (tenant_id, formula, monthly_cap_micros, unlimited) \
         values ($1, $2, $3, $4) \
         on conflict (tenant_id) do update set formula = excluded.formula, \
         monthly_cap_micros = excluded.monthly_cap_micros, unlimited = excluded.unlimited, \
         updated_at = now()",
    )
    .bind(tenant_id)
    .bind(formula.formula.as_str())
    .bind(formula.monthly_cap_micros)
    .bind(formula.unlimited)
    .execute(conn)
    .await?;
    Ok(())
}

/// Sets the company's markup, its formula kept, its own key without one
/// (owner role).
pub async fn set_supply_markup(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    markup_bps: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_model_supply (tenant_id, formula, markup_bps) \
         values ($1, 'client_key', $2) \
         on conflict (tenant_id) do update set markup_bps = excluded.markup_bps, \
         updated_at = now()",
    )
    .bind(tenant_id)
    .bind(markup_bps)
    .execute(conn)
    .await?;
    Ok(())
}

/// Sets the shares the company's administrators are alerted at, those
/// given alone, its formula kept, its own key without one (owner role).
pub async fn set_supply_alerts(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    cap_percents: Option<&[i16]>,
    credit_percents: Option<&[i16]>,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_model_supply (tenant_id, formula, cap_alert_percents, \
         credit_alert_percents) \
         values ($1, 'client_key', coalesce($2, '{80,95}'::smallint[]), \
         coalesce($3, '{20,5}'::smallint[])) \
         on conflict (tenant_id) do update set \
         cap_alert_percents = coalesce($2, tenant_model_supply.cap_alert_percents), \
         credit_alert_percents = coalesce($3, tenant_model_supply.credit_alert_percents), \
         updated_at = now()",
    )
    .bind(tenant_id)
    .bind(cap_percents)
    .bind(credit_percents)
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CreditEntryRow {
    pub amount_micros: i64,
    pub reference: String,
    pub recorded_at: DateTime<Utc>,
}

/// Records a payment of the prepaid credit (owner role); `false` when the
/// company recorded one under that reference already.
pub async fn add_credit_entry(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    amount_micros: i64,
    reference: &str,
) -> sqlx::Result<bool> {
    let added = sqlx::query(
        "insert into tenant_credit_entries (tenant_id, amount_micros, reference) \
         values ($1, $2, $3) on conflict (tenant_id, reference) do nothing",
    )
    .bind(tenant_id)
    .bind(amount_micros)
    .bind(reference)
    .execute(conn)
    .await?;
    Ok(added.rows_affected() == 1)
}

/// The company's payments, the earliest first.
pub async fn credit_entries(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<CreditEntryRow>> {
    sqlx::query_as(
        "select amount_micros, reference, recorded_at from tenant_credit_entries \
         where tenant_id = $1 order by recorded_at, reference",
    )
    .bind(tenant_id)
    .fetch_all(conn)
    .await
}

/// What the company's calls on the operator's key cost since `since` (the
/// start of the month), really and re-invoiced, under each formula; and
/// what its payments and its prepaid calls add up to, ever.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct SupplySpending {
    pub capped_cost_micros: i64,
    pub capped_billed_micros: i64,
    pub prepaid_cost_micros: i64,
    pub prepaid_billed_micros: i64,
    /// Ever, the calls purged after five years included (ADR-197).
    pub prepaid_billed_ever_micros: i64,
    pub credited_micros: i64,
}

impl SupplySpending {
    /// The prepaid credit's balance: payments less prepaid consumption,
    /// computed again at each read.
    pub fn balance_micros(&self) -> i64 {
        self.credited_micros - self.prepaid_billed_ever_micros
    }
}

/// Each part read through its own index: the month's calls by company and
/// time, the prepaid ones ever by the index of the prepaid calls.
pub async fn supply_spending(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    since: DateTime<Utc>,
) -> sqlx::Result<SupplySpending> {
    sqlx::query_as(
        "select \
         coalesce(sum(m.cost_micros) filter (where m.supply_formula = 'capped'), 0)::bigint \
           as capped_cost_micros, \
         coalesce(sum(m.billed_micros) filter (where m.supply_formula = 'capped'), 0)::bigint \
           as capped_billed_micros, \
         coalesce(sum(m.cost_micros) filter (where m.supply_formula = 'prepaid'), 0)::bigint \
           as prepaid_cost_micros, \
         coalesce(sum(m.billed_micros) filter (where m.supply_formula = 'prepaid'), 0)::bigint \
           as prepaid_billed_micros, \
         (coalesce((select sum(u.billed_micros) from usage_records u \
                    where u.tenant_id = $1 and u.supply_formula = 'prepaid'), 0) \
           + coalesce((select p.billed_micros from tenant_prepaid_purged p \
                       where p.tenant_id = $1), 0))::bigint as prepaid_billed_ever_micros, \
         coalesce((select sum(c.amount_micros) from tenant_credit_entries c \
                   where c.tenant_id = $1), 0)::bigint as credited_micros \
         from usage_records m where m.tenant_id = $1 and m.created_at >= $2 \
         and m.supply_formula <> 'client_key'",
    )
    .bind(tenant_id)
    .bind(since)
    .fetch_one(conn)
    .await
}

/// What the company's supply stands at before a call (ADR-208, step 1),
/// read as the application: its formula and cap, and, as the operator's
/// report reads them (`supply_spending`), what its month was re-invoiced
/// under the cap and its credit's balance.
pub async fn supply_state(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    month_start: DateTime<Utc>,
) -> sqlx::Result<SupplyState> {
    let row = model_supply(&mut *conn, tenant_id).await?;
    let spending = supply_spending(conn, tenant_id, month_start).await?;
    // A formula the table does not know is none of the operator's.
    let formula = row
        .as_ref()
        .and_then(|row| SupplyFormula::parse(&row.formula))
        .unwrap_or_default();
    Ok(SupplyState {
        formula,
        monthly_cap_micros: row.as_ref().and_then(|row| row.monthly_cap_micros),
        unlimited: row.is_some_and(|row| row.unlimited),
        capped_month_billed_micros: spending.capped_billed_micros,
        prepaid_balance_micros: spending.balance_micros(),
    })
}
