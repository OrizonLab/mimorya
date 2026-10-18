//! Connector instances configured by each tenant.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

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

pub async fn list_connector_instances(
    conn: &mut PgConnection,
    only_enabled: bool,
) -> sqlx::Result<Vec<ConnectorInstanceRow>> {
    sqlx::query_as(
        "select id, tenant_id, type, name, enabled, config, tool_policies, status, secret_id, last_error, \
         created_at, updated_at from connector_instances where (not $1 or enabled) \
         order by created_at desc",
    )
    .bind(only_enabled)
    .fetch_all(conn)
    .await
}
