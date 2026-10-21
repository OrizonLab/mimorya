//! What tells a manager that a request awaits them (ADR-060): the request,
//! while it waits for them, and their address, while their account is
//! active.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApprovalNotice {
    pub run_id: Uuid,
    pub risk: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub requested_for_user_id: Uuid,
    pub requested_for_name: String,
    pub approver_user_id: Uuid,
    pub approver_email: String,
    pub approver_name: String,
    pub company: String,
}

/// The pending request `approval_id` a manager decides, and that manager,
/// if their account is active; `None` otherwise (decided meanwhile, or
/// decided by whom the agent acts for).
pub async fn approval_notice(
    conn: &mut PgConnection,
    approval_id: Uuid,
) -> sqlx::Result<Option<ApprovalNotice>> {
    sqlx::query_as(
        "select a.run_id, a.risk, a.expires_at, a.requested_for_user_id, \
         r.display_name as requested_for_name, a.approver_user_id, \
         m.email::text as approver_email, m.display_name as approver_name, t.name as company \
         from approvals a \
         join users r on r.id = a.requested_for_user_id \
         join users m on m.id = a.approver_user_id \
         join tenants t on t.id = a.tenant_id \
         where a.id = $1 and a.status = 'pending' and a.routing = 'manager' \
         and m.status = 'active'",
    )
    .bind(approval_id)
    .fetch_optional(conn)
    .await
}
