//! What tells a scheduled task's author that its run awaits their
//! validation (ADR-082): the requests they decide themselves in the run
//! their task last started, while it is paused, their account and company
//! active, not yet notified; at startup, the runs a stop of the service
//! kept them from (ADR-070).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::approval_notices::{NOTIFIED, NoticeRun};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuthorNotice {
    pub approval_id: Uuid,
    pub run_id: Uuid,
    pub risk: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub author_user_id: Uuid,
    pub author_email: String,
    pub author_name: String,
    pub task_title: String,
    /// Where the run is, which the e-mail leads to.
    pub conversation_id: Option<Uuid>,
    pub company: String,
}

/// The run's pending requests its author decides, while it is the run their
/// task last started, paused, in an active company, their account active,
/// none notified yet; in creation order. Completed by a filter on `a`.
const PENDING_FOR_AUTHOR: &str = "select a.id as approval_id, a.run_id, a.risk, a.expires_at, \
     u.id as author_user_id, u.email::text as author_email, u.display_name as author_name, \
     t.title as task_title, ar.conversation_id, c.name as company \
     from approvals a \
     join agent_runs ar on ar.id = a.run_id \
     join scheduled_tasks t on t.last_run_id = a.run_id and t.user_id = a.requested_for_user_id \
     join users u on u.id = a.requested_for_user_id \
     join tenants c on c.id = a.tenant_id \
     where a.run_id = $1 and a.status = 'pending' and a.routing = 'requester' \
     and ar.status = 'awaiting_approval' and c.status = 'active' and u.status = 'active' \
     and not exists (select 1 from audit_log l where l.run_id = a.run_id \
       and l.action = $2 and l.resource_id = a.id::text)";

/// Of the requests `approval_ids` of `run_id`, those to tell its author of.
pub async fn author_notices(
    conn: &mut PgConnection,
    run_id: Uuid,
    approval_ids: &[Uuid],
) -> sqlx::Result<Vec<AuthorNotice>> {
    sqlx::query_as(&format!(
        "{PENDING_FOR_AUTHOR} and a.id = any($3) order by a.created_at"
    ))
    .bind(run_id)
    .bind(NOTIFIED)
    .bind(approval_ids)
    .fetch_all(conn)
    .await
}

/// The requests of `run_id` its author was never told of (ADR-070).
pub async fn author_notices_never_sent(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> sqlx::Result<Vec<AuthorNotice>> {
    sqlx::query_as(&format!("{PENDING_FOR_AUTHOR} order by a.created_at"))
        .bind(run_id)
        .bind(NOTIFIED)
        .fetch_all(conn)
        .await
}

/// Every company's scheduled runs with requests their author was never told
/// of, through a narrowly scoped database function: each is then handled in
/// its own company.
pub async fn unnotified_scheduled_runs(pool: &PgPool) -> sqlx::Result<Vec<NoticeRun>> {
    sqlx::query_as("select tenant_id, run_id from system_unnotified_scheduled_runs()")
        .fetch_all(pool)
        .await
}
