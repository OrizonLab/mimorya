//! Deliverables produced by runs (draft plannings, reports…).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeliverableRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub run_id: Option<Uuid>,
    pub owner_user_id: Uuid,
    pub org_unit_id: Option<Uuid>,
    #[sqlx(rename = "type")]
    pub kind: String,
    pub title: String,
    pub content: Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const DELIVERABLE_COLUMNS: &str = "id, tenant_id, run_id, owner_user_id, org_unit_id, type, title, content, status, created_at, updated_at";

#[derive(Debug, Clone)]
pub struct NewDeliverable {
    pub tenant_id: Uuid,
    pub run_id: Option<Uuid>,
    pub owner_user_id: Uuid,
    pub org_unit_id: Option<Uuid>,
    pub kind: String,
    pub title: String,
    pub content: Value,
}

pub async fn create_deliverable(
    conn: &mut PgConnection,
    input: NewDeliverable,
) -> sqlx::Result<DeliverableRow> {
    sqlx::query_as(&format!(
        "insert into deliverables (tenant_id, run_id, owner_user_id, org_unit_id, type, title, content) \
         values ($1, $2, $3, $4, $5, $6, $7) returning {DELIVERABLE_COLUMNS}"
    ))
    .bind(input.tenant_id)
    .bind(input.run_id)
    .bind(input.owner_user_id)
    .bind(input.org_unit_id)
    .bind(input.kind)
    .bind(input.title)
    .bind(input.content)
    .fetch_one(conn)
    .await
}

pub async fn get_deliverable(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<DeliverableRow>> {
    sqlx::query_as(&format!(
        "select {DELIVERABLE_COLUMNS} from deliverables where id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub async fn set_deliverable_status(
    conn: &mut PgConnection,
    id: Uuid,
    status: &str,
) -> sqlx::Result<()> {
    sqlx::query("update deliverables set status = $2, updated_at = now() where id = $1")
        .bind(id)
        .bind(status)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn list_deliverables(
    conn: &mut PgConnection,
    owner_user_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<DeliverableRow>> {
    sqlx::query_as(&format!(
        "select {DELIVERABLE_COLUMNS} from deliverables where owner_user_id = $1 \
         order by created_at desc limit $2"
    ))
    .bind(owner_user_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}
