//! Connector instances configured by each tenant. A change of configuration,
//! policies or state bumps `updated_at` (the live instance is rebuilt); a
//! status report does not, and only applies to the version it was made on.
//! An instance is `untested` until a start succeeds (`ok`) or fails (`error`).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// The state of an instance (`connector_instances_status_check`).
pub const CONNECTOR_STATUSES: [&str; 4] = ["untested", "ok", "error", "disabled"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConnectorInstanceRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    #[sqlx(rename = "type")]
    pub connector_type: String,
    pub name: String,
    pub enabled: bool,
    pub config: Value,
    pub tool_policies: Value,
    pub status: String,
    pub secret_id: Option<Uuid>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SELECT: &str = "select id, tenant_id, type, name, enabled, config, tool_policies, status, \
    secret_id, last_error, created_at, updated_at from connector_instances";

pub async fn list_connector_instances(
    conn: &mut PgConnection,
    only_enabled: bool,
) -> sqlx::Result<Vec<ConnectorInstanceRow>> {
    sqlx::query_as(&format!(
        "{SELECT} where (not $1 or enabled) order by created_at desc"
    ))
    .bind(only_enabled)
    .fetch_all(conn)
    .await
}

pub async fn get_connector_instance(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<ConnectorInstanceRow>> {
    sqlx::query_as(&format!("{SELECT} where id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

pub struct NewConnectorInstance<'a> {
    pub tenant_id: Uuid,
    pub connector_type: &'a str,
    pub name: &'a str,
    pub enabled: bool,
    pub config: &'a Value,
}

pub async fn create_connector_instance(
    conn: &mut PgConnection,
    input: NewConnectorInstance<'_>,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into connector_instances (tenant_id, type, name, enabled, config, status) \
         values ($1, $2, $3, $4, $5, case when $4 then 'untested' else 'disabled' end) \
         returning id",
    )
    .bind(input.tenant_id)
    .bind(input.connector_type)
    .bind(input.name)
    .bind(input.enabled)
    .bind(input.config)
    .fetch_one(conn)
    .await
}

/// Fields left to `None` are unchanged.
#[derive(Default)]
pub struct ConnectorPatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<Value>,
    pub tool_policies: Option<Value>,
    /// The address or the token changed: what was known of the server no
    /// longer holds.
    pub reachability_changed: bool,
}

/// Returns whether the instance exists (in the transaction's tenant). A
/// disabled instance reads `disabled`; one re-enabled, or whose address or
/// token changed, reads `untested` again.
pub async fn update_connector_instance(
    conn: &mut PgConnection,
    id: Uuid,
    patch: ConnectorPatch,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update connector_instances set name = coalesce($2, name), \
         enabled = coalesce($3, enabled), config = coalesce($4, config), \
         tool_policies = coalesce($5, tool_policies), \
         status = case when not coalesce($3, enabled) then 'disabled' \
           when $6 or status = 'disabled' then 'untested' else status end, \
         last_error = case when coalesce($3, enabled) and ($6 or status = 'disabled') \
           then null else last_error end, \
         updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(patch.name)
    .bind(patch.enabled)
    .bind(patch.config)
    .bind(patch.tool_policies)
    .bind(patch.reachability_changed)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_connector_instance(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from connector_instances where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Records how a start of the version `updated_at` went, without rebuilding
/// the live instance; ignored once the instance changed or was disabled.
pub async fn report_connector_status(
    conn: &mut PgConnection,
    id: Uuid,
    updated_at: DateTime<Utc>,
    error: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update connector_instances set status = case when $3::text is null then 'ok' else 'error' end, \
         last_error = $3 where id = $1 and updated_at = $2 and enabled",
    )
    .bind(id)
    .bind(updated_at)
    .bind(error)
    .execute(conn)
    .await?;
    Ok(())
}
