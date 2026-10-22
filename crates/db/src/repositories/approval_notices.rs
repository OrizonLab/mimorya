//! What tells a manager that requests await them (ADR-060), at startup if a
//! stop of the service lost the notice (ADR-070), or reminds them the day
//! before they expire (ADR-062): the requests that still wait for a manager,
//! with the manager's address while their account is active, not yet
//! notified (or reminded).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
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

/// The audited action of a request's first notice (ADR-060).
pub const NOTIFIED: &str = "approval.notified";
/// The audited action of a request's reminder (ADR-062).
pub const REMINDED: &str = "approval.reminded";

/// The run's pending requests a manager decides, while the run is paused
/// and its company active, with that manager, if their account is active,
/// and none of the audited `$2` action yet; by manager, then in creation
/// order. Completed by a filter on `a`.
const PENDING_FOR_MANAGERS: &str = "select a.id as approval_id, a.run_id, a.risk, a.expires_at, \
     a.requested_for_user_id, r.display_name as requested_for_name, a.approver_user_id, \
     m.email::text as approver_email, m.display_name as approver_name, t.name as company \
     from approvals a \
     join agent_runs ar on ar.id = a.run_id \
     join users r on r.id = a.requested_for_user_id \
     join users m on m.id = a.approver_user_id \
     join tenants t on t.id = a.tenant_id \
     where a.run_id = $1 and a.status = 'pending' and a.routing = 'manager' \
     and ar.status = 'awaiting_approval' and t.status = 'active' and m.status = 'active' \
     and not exists (select 1 from audit_log l where l.run_id = a.run_id \
       and l.action = $2 and l.resource_id = a.id::text)";

const BY_MANAGER: &str = " order by a.approver_user_id, a.created_at";

/// Of the requests `approval_ids` of `run_id`, those to notify: no notice
/// attempted for yet (sent or refused by the relay: it is never retried,
/// ADR-060).
pub async fn approval_notices(
    conn: &mut PgConnection,
    run_id: Uuid,
    approval_ids: &[Uuid],
) -> sqlx::Result<Vec<ApprovalNotice>> {
    sqlx::query_as(&format!(
        "{PENDING_FOR_MANAGERS} and a.id = any($3){BY_MANAGER}"
    ))
    .bind(run_id)
    .bind(NOTIFIED)
    .bind(approval_ids)
    .fetch_all(conn)
    .await
}

/// The requests of `run_id` whose manager was never told of (ADR-070): no
/// notice attempted for, due after `due_after`.
pub async fn approvals_never_notified(
    conn: &mut PgConnection,
    run_id: Uuid,
    due_after: DateTime<Utc>,
) -> sqlx::Result<Vec<ApprovalNotice>> {
    sqlx::query_as(&format!(
        "{PENDING_FOR_MANAGERS} and a.expires_at > $3{BY_MANAGER}"
    ))
    .bind(run_id)
    .bind(NOTIFIED)
    .bind(due_after)
    .fetch_all(conn)
    .await
}

/// The requests of `run_id` to remind (ADR-062): due after `now` but by
/// `until`, never reminded yet.
pub async fn approvals_to_remind(
    conn: &mut PgConnection,
    run_id: Uuid,
    now: DateTime<Utc>,
    until: DateTime<Utc>,
) -> sqlx::Result<Vec<ApprovalNotice>> {
    sqlx::query_as(&format!(
        "{PENDING_FOR_MANAGERS} and a.expires_at > $3 and a.expires_at <= $4{BY_MANAGER}"
    ))
    .bind(run_id)
    .bind(REMINDED)
    .bind(now)
    .bind(until)
    .fetch_all(conn)
    .await
}

/// A run with requests to tell their manager of (ADR-062, ADR-070).
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct NoticeRun {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
}

/// Every company's runs with requests due after `now` but by `until` that
/// a manager has not been reminded of, through a narrowly scoped database
/// function: each is then handled in its own company.
pub async fn approval_reminder_runs(
    pool: &PgPool,
    now: DateTime<Utc>,
    until: DateTime<Utc>,
) -> sqlx::Result<Vec<NoticeRun>> {
    sqlx::query_as("select tenant_id, run_id from system_approval_reminder_runs($1, $2)")
        .bind(now)
        .bind(until)
        .fetch_all(pool)
        .await
}

/// Every company's runs with requests due after `due_after` that their
/// manager was never told of (ADR-070), through a narrowly scoped database
/// function: each is then handled in its own company.
pub async fn unnotified_approval_runs(
    pool: &PgPool,
    due_after: DateTime<Utc>,
) -> sqlx::Result<Vec<NoticeRun>> {
    sqlx::query_as("select tenant_id, run_id from system_unnotified_approval_runs($1)")
        .bind(due_after)
        .fetch_all(pool)
        .await
}
