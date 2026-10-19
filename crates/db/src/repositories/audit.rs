//! Append-only audit trail. Never store secrets or message contents here.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// Who acted (`audit_log_actor_check`).
pub const AUDIT_ACTORS: [&str; 3] = ["user", "agent", "system"];
/// How it ended (`audit_log_outcome_check`).
pub const AUDIT_OUTCOMES: [&str; 3] = ["success", "denied", "failure"];

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub tenant_id: Uuid,
    /// `user`, `agent` or `system`.
    pub actor_type: &'static str,
    pub actor_user_id: Option<Uuid>,
    pub on_behalf_of_user_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    /// `success`, `denied` or `failure`.
    pub outcome: &'static str,
    pub reason: Option<String>,
    pub metadata: Value,
    pub ip: Option<String>,
    pub request_id: Option<String>,
}

impl AuditEntry {
    pub fn new(
        tenant_id: Uuid,
        actor_type: &'static str,
        action: impl Into<String>,
        outcome: &'static str,
    ) -> Self {
        Self {
            tenant_id,
            actor_type,
            actor_user_id: None,
            on_behalf_of_user_id: None,
            run_id: None,
            action: action.into(),
            resource_type: None,
            resource_id: None,
            outcome,
            reason: None,
            metadata: json!({}),
            ip: None,
            request_id: None,
        }
    }

    pub fn by_user(mut self, user_id: Uuid) -> Self {
        self.actor_user_id = Some(user_id);
        self
    }

    pub fn on_behalf_of(mut self, user_id: Uuid) -> Self {
        self.on_behalf_of_user_id = Some(user_id);
        self
    }

    pub fn run(mut self, run_id: Uuid) -> Self {
        self.run_id = Some(run_id);
        self
    }

    pub fn resource(mut self, resource_type: &str, resource_id: impl ToString) -> Self {
        self.resource_type = Some(resource_type.into());
        self.resource_id = Some(resource_id.to_string());
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn ip(mut self, ip: Option<String>) -> Self {
        self.ip = ip;
        self
    }
}

pub async fn append_audit(conn: &mut PgConnection, entry: AuditEntry) -> sqlx::Result<()> {
    sqlx::query(
        "insert into audit_log (tenant_id, actor_type, actor_user_id, on_behalf_of_user_id, run_id, \
         action, resource_type, resource_id, outcome, reason, metadata, ip, request_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12::inet, $13)",
    )
    .bind(entry.tenant_id)
    .bind(entry.actor_type)
    .bind(entry.actor_user_id)
    .bind(entry.on_behalf_of_user_id)
    .bind(entry.run_id)
    .bind(entry.action)
    .bind(entry.resource_type)
    .bind(entry.resource_id)
    .bind(entry.outcome)
    .bind(entry.reason)
    .bind(entry.metadata)
    .bind(entry.ip)
    .bind(entry.request_id)
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditRow {
    pub id: i64,
    pub occurred_at: DateTime<Utc>,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub on_behalf_of_user_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub outcome: String,
    pub reason: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub before_id: Option<i64>,
    pub run_id: Option<Uuid>,
}

const MAX_AUDIT_PAGE: i64 = 500;

pub async fn list_audit(conn: &mut PgConnection, query: AuditQuery) -> sqlx::Result<Vec<AuditRow>> {
    sqlx::query_as(
        "select id, occurred_at, actor_type, actor_user_id, on_behalf_of_user_id, run_id, action, \
         resource_type, resource_id, outcome, reason, metadata from audit_log \
         where ($1::bigint is null or id < $1) and ($2::uuid is null or run_id = $2) \
         order by id desc limit $3",
    )
    .bind(query.before_id)
    .bind(query.run_id)
    .bind(query.limit.unwrap_or(100).min(MAX_AUDIT_PAGE))
    .fetch_all(conn)
    .await
}
