//! A company's model escalation (ADR-207): its stronger model, the
//! mechanisms that call it, the router's rule for each kind of run, and
//! their bounds; and what those mechanisms
//! have cost since a moment, read from the usage records of the roles the
//! engine says its budget counts.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ModelEscalationRow {
    pub provider: String,
    pub base_url: Option<String>,
    pub model: String,
    pub after_failure: bool,
    pub router: bool,
    pub advisor: bool,
    /// The router's rules (`EscalationRoute::as_str`), for a conversation,
    /// a mission's stage, a scheduled task's run and a hook's.
    pub route_conversation: String,
    pub route_mission: String,
    pub route_scheduled_task: String,
    pub route_hook: String,
    pub monthly_budget_micros: i64,
    pub max_per_run: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewModelEscalation {
    pub tenant_id: Uuid,
    pub provider: String,
    pub base_url: Option<String>,
    pub model: String,
    pub after_failure: bool,
    pub router: bool,
    pub advisor: bool,
    pub route_conversation: &'static str,
    pub route_mission: &'static str,
    pub route_scheduled_task: &'static str,
    pub route_hook: &'static str,
    pub monthly_budget_micros: i64,
    pub max_per_run: i32,
    pub updated_by_user_id: Uuid,
}

/// The company's escalation, if it chose one.
pub async fn model_escalation(conn: &mut PgConnection) -> sqlx::Result<Option<ModelEscalationRow>> {
    sqlx::query_as(
        "select provider, base_url, model, after_failure, router, advisor, \
         route_conversation, route_mission, route_scheduled_task, route_hook, \
         monthly_budget_micros, max_per_run, updated_at from tenant_model_escalation",
    )
    .fetch_optional(conn)
    .await
}

pub async fn set_model_escalation(
    conn: &mut PgConnection,
    escalation: NewModelEscalation,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_model_escalation (tenant_id, provider, base_url, model, \
         after_failure, router, advisor, route_conversation, route_mission, \
         route_scheduled_task, route_hook, monthly_budget_micros, max_per_run, \
         updated_by_user_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
         on conflict (tenant_id) do update set provider = excluded.provider, \
         base_url = excluded.base_url, model = excluded.model, \
         after_failure = excluded.after_failure, router = excluded.router, \
         advisor = excluded.advisor, route_conversation = excluded.route_conversation, \
         route_mission = excluded.route_mission, \
         route_scheduled_task = excluded.route_scheduled_task, \
         route_hook = excluded.route_hook, \
         monthly_budget_micros = excluded.monthly_budget_micros, \
         max_per_run = excluded.max_per_run, updated_by_user_id = excluded.updated_by_user_id, \
         updated_at = now()",
    )
    .bind(escalation.tenant_id)
    .bind(escalation.provider)
    .bind(escalation.base_url)
    .bind(escalation.model)
    .bind(escalation.after_failure)
    .bind(escalation.router)
    .bind(escalation.advisor)
    .bind(escalation.route_conversation)
    .bind(escalation.route_mission)
    .bind(escalation.route_scheduled_task)
    .bind(escalation.route_hook)
    .bind(escalation.monthly_budget_micros)
    .bind(escalation.max_per_run)
    .bind(escalation.updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes the company's escalation. Whether there was one.
pub async fn remove_model_escalation(conn: &mut PgConnection) -> sqlx::Result<bool> {
    let removed = sqlx::query("delete from tenant_model_escalation")
        .execute(conn)
        .await?;
    Ok(removed.rows_affected() > 0)
}

/// What the model calls of `roles` have cost since `since`, in
/// micro-dollars.
pub async fn escalation_spent(
    conn: &mut PgConnection,
    since: DateTime<Utc>,
    roles: &[&str],
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select coalesce(sum(cost_micros), 0)::bigint from usage_records \
         where created_at >= $1 and role = any($2)",
    )
    .bind(since)
    .bind(roles)
    .fetch_one(conn)
    .await
}
