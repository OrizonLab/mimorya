//! Log of tool calls made by runs, with policy decisions and outcomes.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewToolInvocation {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Option<Value>,
    /// `allow`, `require_approval` or `deny`.
    pub decision: String,
    pub decision_reason: Option<String>,
    /// `succeeded`, `failed`, `denied`, `awaiting_approval` or `rejected`.
    pub status: String,
    pub output_summary: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<i32>,
    pub approval_id: Option<Uuid>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ToolInvocationRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub tool_call_id: String,
    pub tool_name: String,
    pub decision: String,
    pub decision_reason: Option<String>,
    pub status: String,
    pub output_summary: Option<String>,
    pub duration_ms: Option<i32>,
    pub approval_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub async fn record_tool_invocation(
    conn: &mut PgConnection,
    input: NewToolInvocation,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tool_invocations (tenant_id, run_id, tool_call_id, tool_name, input, decision, \
         decision_reason, status, output_summary, error, duration_ms, approval_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(input.tenant_id)
    .bind(input.run_id)
    .bind(input.tool_call_id)
    .bind(input.tool_name)
    .bind(input.input)
    .bind(input.decision)
    .bind(input.decision_reason)
    .bind(input.status)
    .bind(input.output_summary)
    .bind(input.error)
    .bind(input.duration_ms)
    .bind(input.approval_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn list_tool_invocations(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> sqlx::Result<Vec<ToolInvocationRow>> {
    sqlx::query_as(
        "select id, run_id, tool_call_id, tool_name, decision, decision_reason, status, output_summary, \
         duration_ms, approval_id, created_at from tool_invocations where run_id = $1 order by created_at",
    )
    .bind(run_id)
    .fetch_all(conn)
    .await
}
