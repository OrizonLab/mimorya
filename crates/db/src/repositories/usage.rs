//! Usage metering (model calls, embeddings, tools) and summaries.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// What was used (`usage_records_kind_check`).
pub const USAGE_KINDS: [&str; 3] = ["llm", "embedding", "tool"];

/// The role of a model call (`usage_records_role_check`, ADR-207): the main
/// model, the escalation model, a classification, the advisor, or the
/// review of an answer. Other uses are the main model's.
pub const USAGE_ROLES: [&str; 5] = ["main", "escalation", "classification", "advisor", "review"];

#[derive(Debug, Clone)]
pub struct NewUsage {
    pub tenant_id: Uuid,
    pub user_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    /// `llm`, `embedding` or `tool`.
    pub kind: &'static str,
    /// One of `USAGE_ROLES`.
    pub role: &'static str,
    pub provider: String,
    pub model: String,
    pub input_tokens: i32,
    pub output_tokens: i32,
    /// Unknown prices are recorded as 0 (and flagged in the run events).
    pub cost_micros: i64,
}

pub async fn append_usage(conn: &mut PgConnection, entry: NewUsage) -> sqlx::Result<()> {
    sqlx::query(
        "insert into usage_records (tenant_id, user_id, run_id, kind, role, provider, model, \
         input_tokens, output_tokens, cost_micros) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(entry.tenant_id)
    .bind(entry.user_id)
    .bind(entry.run_id)
    .bind(entry.kind)
    .bind(entry.role)
    .bind(entry.provider)
    .bind(entry.model)
    .bind(entry.input_tokens)
    .bind(entry.output_tokens)
    .bind(entry.cost_micros)
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UsageSummaryRow {
    pub provider: String,
    pub model: String,
    pub kind: String,
    pub role: String,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micros: i64,
}

pub async fn usage_summary(
    conn: &mut PgConnection,
    since: DateTime<Utc>,
) -> sqlx::Result<Vec<UsageSummaryRow>> {
    sqlx::query_as(
        "select provider, model, kind, role, count(*) as calls, \
         coalesce(sum(input_tokens), 0)::bigint as input_tokens, \
         coalesce(sum(output_tokens), 0)::bigint as output_tokens, \
         coalesce(sum(cost_micros), 0)::bigint as cost_micros \
         from usage_records where created_at >= $1 group by provider, model, kind, role",
    )
    .bind(since)
    .fetch_all(conn)
    .await
}
