//! The alerts of the operator's supply (ADR-211): the companies on the
//! operator's key, which the pass reads each in its own company, and the
//! alerts already sent for a month of the cap or a payment of the credit,
//! recorded as they are sent, never changed.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Every active company on the operator's key (formula 2 or 3), through a
/// narrowly scoped database function.
pub async fn supplied_companies(pool: &PgPool) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("select tenant_id from system_supplied_companies()")
        .fetch_all(pool)
        .await
}

/// The shares of `kind` already alerted for `period`.
pub async fn sent_supply_alerts(
    conn: &mut PgConnection,
    kind: &str,
    period: &str,
) -> sqlx::Result<Vec<i16>> {
    sqlx::query_scalar("select percent from tenant_supply_alerts where kind = $1 and period = $2")
        .bind(kind)
        .bind(period)
        .fetch_all(conn)
        .await
}

/// Records the shares of `kind` alerted for `period`: none is sent again.
pub async fn record_supply_alerts(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    kind: &str,
    period: &str,
    percents: &[i16],
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_supply_alerts (tenant_id, kind, period, percent) \
         select $1, $2, $3, unnest($4::smallint[]) on conflict do nothing",
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(period)
    .bind(percents)
    .execute(conn)
    .await?;
    Ok(())
}
