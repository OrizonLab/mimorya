//! What the assistant did for one person, as the export of their own data
//! reads it (ADR-187): the runs made on their behalf and the tools each
//! called, the validations asked of them for their own runs, those they
//! gave on others' (their decision alone, never the other's request), what
//! their runs delivered, and what the assistant consumed for them, month
//! by month. Read in the person's company's transaction, for the person
//! alone.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedRun {
    pub id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub parent_run_id: Option<Uuid>,
    pub objective: String,
    pub status: String,
    pub channel: String,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedToolCall {
    pub run_id: Uuid,
    pub tool_name: String,
    pub decision: String,
    pub status: String,
    pub input: Option<Value>,
    pub created_at: DateTime<Utc>,
}

/// A validation asked of the person for one of their own runs.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedApproval {
    pub run_id: Uuid,
    pub tool_name: String,
    pub action_summary: String,
    pub payload: Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_comment: Option<String>,
}

/// A decision the person gave on someone else's run: theirs alone.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedDecision {
    pub tool_name: String,
    pub status: String,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_comment: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedDeliverable {
    pub kind: String,
    pub title: String,
    pub content: Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// What the assistant consumed for the person in a month, of one kind.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedUsage {
    pub month: DateTime<Utc>,
    pub kind: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micros: i64,
}

pub async fn exported_runs(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedRun>> {
    sqlx::query_as(
        "select id, conversation_id, parent_run_id, objective, status, channel, created_at, \
         finished_at from agent_runs where on_behalf_of_user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_tool_calls(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedToolCall>> {
    sqlx::query_as(
        "select t.run_id, t.tool_name, t.decision, t.status, t.input, t.created_at \
         from tool_invocations t \
         join agent_runs r on r.tenant_id = t.tenant_id and r.id = t.run_id \
         where r.on_behalf_of_user_id = $1 order by t.created_at, t.id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_approvals(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedApproval>> {
    sqlx::query_as(
        "select run_id, tool_name, action_summary, payload, status, created_at, decided_at, \
         decision_comment from approvals where requested_for_user_id = $1 \
         order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_decisions(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedDecision>> {
    sqlx::query_as(
        "select tool_name, status, decided_at, decision_comment from approvals \
         where decided_by_user_id = $1 and requested_for_user_id <> $1 \
         order by decided_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_deliverables(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedDeliverable>> {
    sqlx::query_as(
        "select type as kind, title, content, status, created_at from deliverables \
         where owner_user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_usage(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedUsage>> {
    sqlx::query_as(
        "select date_trunc('month', created_at) as month, kind, \
         sum(input_tokens)::bigint as input_tokens, sum(output_tokens)::bigint as output_tokens, \
         sum(cost_micros)::bigint as cost_micros from usage_records where user_id = $1 \
         group by 1, 2 order by 1, 2",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}
