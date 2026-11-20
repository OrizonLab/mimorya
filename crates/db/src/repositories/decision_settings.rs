//! A company's decision service (ADR-213): the engine it chose, and the
//! mode and threshold of each usage it set.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DecisionSettingsRow {
    /// `main_model` or `jev`.
    pub engine: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct DecisionUsageRow {
    pub usage: String,
    pub mode: String,
    pub threshold: f64,
}

/// The company's engine, if it chose one.
pub async fn decision_settings(
    conn: &mut PgConnection,
) -> sqlx::Result<Option<DecisionSettingsRow>> {
    sqlx::query_as("select engine, updated_at from tenant_decision_settings")
        .fetch_optional(conn)
        .await
}

/// The usages it set, by name.
pub async fn decision_usages(conn: &mut PgConnection) -> sqlx::Result<Vec<DecisionUsageRow>> {
    sqlx::query_as("select usage, mode, threshold from tenant_decision_usages order by usage")
        .fetch_all(conn)
        .await
}

/// Its engine and every usage, as chosen: the usages not given are left
/// as they were.
pub async fn set_decision_settings(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    engine: &str,
    usages: &[DecisionUsageRow],
    updated_by: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_decision_settings (tenant_id, engine, updated_by_user_id) \
         values ($1, $2, $3) on conflict (tenant_id) do update set engine = excluded.engine, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(engine)
    .bind(updated_by)
    .execute(&mut *conn)
    .await?;
    for usage in usages {
        sqlx::query(
            "insert into tenant_decision_usages (tenant_id, usage, mode, threshold) \
             values ($1, $2, $3, $4) on conflict (tenant_id, usage) do update set \
             mode = excluded.mode, threshold = excluded.threshold",
        )
        .bind(tenant_id)
        .bind(&usage.usage)
        .bind(&usage.mode)
        .bind(usage.threshold)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Back to the main model, every usage off: whether anything was set.
pub async fn remove_decision_settings(conn: &mut PgConnection) -> sqlx::Result<bool> {
    let engine = sqlx::query("delete from tenant_decision_settings")
        .execute(&mut *conn)
        .await?
        .rows_affected();
    let usages = sqlx::query("delete from tenant_decision_usages")
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(engine + usages > 0)
}
