//! A company's own mission limits (ADR-093): stages and cost a mission may
//! take; the platform's without a row.

use sqlx::PgConnection;
use uuid::Uuid;

/// The company's limits, as set: stages and cost, in micro-dollars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct MissionSettingsRow {
    pub max_stages: i32,
    pub max_cost_micros: i64,
}

/// The company's limits, if it set some.
pub async fn mission_settings(conn: &mut PgConnection) -> sqlx::Result<Option<MissionSettingsRow>> {
    sqlx::query_as("select max_stages, max_cost_micros from tenant_mission_settings")
        .fetch_optional(conn)
        .await
}

pub async fn set_mission_settings(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    limits: MissionSettingsRow,
    updated_by_user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_mission_settings (tenant_id, max_stages, max_cost_micros, \
         updated_by_user_id) values ($1, $2, $3, $4) \
         on conflict (tenant_id) do update set max_stages = excluded.max_stages, \
         max_cost_micros = excluded.max_cost_micros, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(limits.max_stages)
    .bind(limits.max_cost_micros)
    .bind(updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes the company's own limits: the platform's apply. Whether there
/// were some.
pub async fn remove_mission_settings(conn: &mut PgConnection) -> sqlx::Result<bool> {
    let removed = sqlx::query("delete from tenant_mission_settings")
        .execute(conn)
        .await?;
    Ok(removed.rows_affected() > 0)
}
