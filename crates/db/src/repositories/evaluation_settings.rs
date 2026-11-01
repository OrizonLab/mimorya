//! A company's own limit on what its skill evaluations may cost over
//! thirty days (ADR-154); the platform's without a row.

use sqlx::PgConnection;
use uuid::Uuid;

/// The company's limit, in micro-dollars, if it set one.
pub async fn evaluation_limit(conn: &mut PgConnection) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("select max_cost_micros from tenant_evaluation_settings")
        .fetch_optional(conn)
        .await
}

pub async fn set_evaluation_limit(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    max_cost_micros: i64,
    updated_by_user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_evaluation_settings (tenant_id, max_cost_micros, updated_by_user_id) \
         values ($1, $2, $3) \
         on conflict (tenant_id) do update set max_cost_micros = excluded.max_cost_micros, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(max_cost_micros)
    .bind(updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes the company's own limit: the platform's applies. Whether there
/// was one.
pub async fn remove_evaluation_limit(conn: &mut PgConnection) -> sqlx::Result<bool> {
    let removed = sqlx::query("delete from tenant_evaluation_settings")
        .execute(conn)
        .await?;
    Ok(removed.rows_affected() > 0)
}
