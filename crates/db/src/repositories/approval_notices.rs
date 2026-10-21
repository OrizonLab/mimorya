//! What tells a manager that requests await them (ADR-060): the requests a
//! run paused on that still wait for a manager, not yet notified, with the
//! manager's address while their account is active.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApprovalNotice {
    pub approval_id: Uuid,
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

/// Of the requests `approval_ids` of `run_id`, those still pending that a
/// manager decides, with that manager, if their account is active, and
/// that no notice was sent for yet; by manager, then in creation order.
pub async fn approval_notices(
    conn: &mut PgConnection,
    run_id: Uuid,
    approval_ids: &[Uuid],
) -> sqlx::Result<Vec<ApprovalNotice>> {
    sqlx::query_as(
        "select a.id as approval_id, a.run_id, a.risk, a.expires_at, a.requested_for_user_id, \
         r.display_name as requested_for_name, a.approver_user_id, \
         m.email::text as approver_email, m.display_name as approver_name, t.name as company \
         from approvals a \
         join users r on r.id = a.requested_for_user_id \
         join users m on m.id = a.approver_user_id \
         join tenants t on t.id = a.tenant_id \
         where a.run_id = $1 and a.id = any($2) and a.status = 'pending' \
         and a.routing = 'manager' and m.status = 'active' \
         and not exists (select 1 from audit_log l where l.run_id = a.run_id \
           and l.action = 'approval.notified' and l.resource_id = a.id::text) \
         order by a.approver_user_id, a.created_at",
    )
    .bind(run_id)
    .bind(approval_ids)
    .fetch_all(conn)
    .await
}
