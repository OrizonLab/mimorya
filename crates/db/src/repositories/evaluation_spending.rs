//! What the evaluations of a company's skills cost, day by day (ADR-154):
//! each case played adds its cost as it is recorded, kept apart from the
//! evaluations, which a skill prunes, so that pruning lowers nothing. The
//! days are UTC days.

use sqlx::PgConnection;
use uuid::Uuid;

/// Adds `cost_micros` to the company's spending of today, and deletes its
/// days older than `kept_days`.
pub async fn add_evaluation_spending(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    cost_micros: i64,
    kept_days: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into skill_evaluation_spending (tenant_id, day, cost_micros) \
         values ($1, (now() at time zone 'utc')::date, $2) \
         on conflict (tenant_id, day) \
         do update set cost_micros = skill_evaluation_spending.cost_micros + excluded.cost_micros",
    )
    .bind(tenant_id)
    .bind(cost_micros.max(0))
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "delete from skill_evaluation_spending \
         where day < (now() at time zone 'utc')::date - $1",
    )
    .bind(kept_days)
    .execute(conn)
    .await?;
    Ok(())
}

/// What the company's evaluations cost over the last `days` days, today
/// included.
pub async fn evaluation_spending(conn: &mut PgConnection, days: i32) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select coalesce(sum(cost_micros), 0)::bigint from skill_evaluation_spending \
         where day > (now() at time zone 'utc')::date - $1",
    )
    .bind(days)
    .fetch_one(conn)
    .await
}
