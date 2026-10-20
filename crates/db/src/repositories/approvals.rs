//! Human validations: creation, listings, decisions (at most once) and expiry.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

pub const APPROVAL_STATUSES: [&str; 5] =
    ["pending", "approved", "rejected", "expired", "cancelled"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApprovalRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    pub requested_for_user_id: Uuid,
    pub tool_name: String,
    pub tool_call_id: String,
    pub action_summary: String,
    pub payload: Value,
    pub risk: String,
    pub reason: String,
    pub status: String,
    pub decided_by_user_id: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_comment: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

const APPROVAL_COLUMNS: &str = "id, tenant_id, run_id, requested_for_user_id, tool_name, tool_call_id, \
    action_summary, payload, risk, reason, status, decided_by_user_id, decided_at, decision_comment, \
    expires_at, created_at";

#[derive(Debug, Clone)]
pub struct NewApproval {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    pub requested_for_user_id: Uuid,
    pub tool_name: String,
    pub tool_call_id: String,
    pub action_summary: String,
    pub payload: Value,
    pub risk: String,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn create_approval(
    conn: &mut PgConnection,
    input: NewApproval,
) -> sqlx::Result<ApprovalRow> {
    sqlx::query_as(&format!(
        "insert into approvals (tenant_id, run_id, requested_for_user_id, tool_name, tool_call_id, \
         action_summary, payload, risk, reason, expires_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) returning {APPROVAL_COLUMNS}"
    ))
    .bind(input.tenant_id)
    .bind(input.run_id)
    .bind(input.requested_for_user_id)
    .bind(input.tool_name)
    .bind(input.tool_call_id)
    .bind(input.action_summary)
    .bind(input.payload)
    .bind(input.risk)
    .bind(input.reason)
    .bind(input.expires_at)
    .fetch_one(conn)
    .await
}

pub async fn get_approval(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<ApprovalRow>> {
    sqlx::query_as(&format!(
        "select {APPROVAL_COLUMNS} from approvals where id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub async fn list_approvals_for_user(
    conn: &mut PgConnection,
    user_id: Uuid,
    statuses: &[&str],
) -> sqlx::Result<Vec<ApprovalRow>> {
    sqlx::query_as(&format!(
        "select {APPROVAL_COLUMNS} from approvals where requested_for_user_id = $1 \
         and status = any($2) order by created_at desc limit 100"
    ))
    .bind(user_id)
    .bind(statuses)
    .fetch_all(conn)
    .await
}

pub async fn list_approvals_for_run(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> sqlx::Result<Vec<ApprovalRow>> {
    sqlx::query_as(&format!(
        "select {APPROVAL_COLUMNS} from approvals where run_id = $1 order by created_at"
    ))
    .bind(run_id)
    .fetch_all(conn)
    .await
}

/// The run's requests still waiting for a decision are cancelled: they
/// belonged to a step a stop of the service lost (ADR-053). Returns them.
pub async fn cancel_pending_approvals(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> sqlx::Result<Vec<ApprovalRow>> {
    sqlx::query_as(&format!(
        "update approvals set status = 'cancelled' where run_id = $1 and status = 'pending' \
         returning {APPROVAL_COLUMNS}"
    ))
    .bind(run_id)
    .fetch_all(conn)
    .await
}

pub async fn expire_approval(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    sqlx::query("update approvals set status = 'expired' where id = $1 and status = 'pending'")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ApprovalDecisionInput {
    pub id: Uuid,
    pub approved: bool,
    pub decided_by_user_id: Uuid,
    pub comment: Option<String>,
}

/// Records a decision only if the approval is still pending (no double decision).
pub async fn decide_approval(
    conn: &mut PgConnection,
    input: ApprovalDecisionInput,
) -> sqlx::Result<Option<ApprovalRow>> {
    sqlx::query_as(&format!(
        "update approvals set status = $2, decided_by_user_id = $3, decided_at = now(), \
         decision_comment = $4 where id = $1 and status = 'pending' returning {APPROVAL_COLUMNS}"
    ))
    .bind(input.id)
    .bind(if input.approved {
        "approved"
    } else {
        "rejected"
    })
    .bind(input.decided_by_user_id)
    .bind(input.comment)
    .fetch_optional(conn)
    .await
}
