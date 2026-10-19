//! Replayable journal of run events (the sequence number doubles as SSE event id).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::runs::RunStatus;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunEventRow {
    pub id: i64,
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    pub seq: i32,
    #[sqlx(rename = "type")]
    pub event_type: String,
    pub data: Value,
    pub created_at: DateTime<Utc>,
}

pub async fn append_run_event(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    run_id: Uuid,
    event_type: &str,
    data: &Value,
) -> sqlx::Result<RunEventRow> {
    // Sequence numbers are per run (one writer per run in practice).
    sqlx::query_as(
        "insert into run_events (tenant_id, run_id, type, data, seq) \
         values ($1, $2, $3, $4, (select coalesce(max(seq), 0) + 1 from run_events where run_id = $2)) \
         returning id, tenant_id, run_id, seq, type, data, created_at",
    )
    .bind(tenant_id)
    .bind(run_id)
    .bind(event_type)
    .bind(data)
    .fetch_one(conn)
    .await
}

/// Whether the run's end is already in its journal: a `run_finished` event
/// with a final status (a pause for validation is not an end).
pub async fn run_end_recorded(conn: &mut PgConnection, run_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select exists (select 1 from run_events where run_id = $1 and type = 'run_finished' \
         and data->>'status' = any($2))",
    )
    .bind(run_id)
    .bind(RunStatus::finished_values())
    .fetch_one(conn)
    .await
}

pub async fn list_run_events(
    conn: &mut PgConnection,
    run_id: Uuid,
    after_seq: i32,
) -> sqlx::Result<Vec<RunEventRow>> {
    sqlx::query_as(
        "select id, tenant_id, run_id, seq, type, data, created_at from run_events \
         where run_id = $1 and seq > $2 order by seq",
    )
    .bind(run_id)
    .bind(after_seq)
    .fetch_all(conn)
    .await
}
