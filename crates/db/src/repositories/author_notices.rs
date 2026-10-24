//! What tells the author of a run nobody watches that it awaits their
//! validation: a scheduled task's run (ADR-082), or a stage of their mission
//! (ADR-091). The requests they decide themselves in the run their task
//! last started, or in the stage of their mission under way, while it is
//! paused, their account and company active, not yet notified; later, those
//! a stop of the service kept from them (ADR-070, ADR-086), not expired yet.

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
    /// The title of the task, or of the mission, that started the run.
    pub title: String,
    /// Whether the run is a stage of a mission, not a scheduled task's.
    pub of_mission: bool,
    /// Where the run is, which the e-mail leads to.
    pub conversation_id: Option<Uuid>,
    pub company: String,
}

/// The run's pending requests its author decides, while it is the run their
/// task last started or a stage of their mission under way, paused, in an
/// active company, their account active, none notified yet; in creation
/// order. Completed by a filter on `a`.
const PENDING_FOR_AUTHOR: &str = "select a.id as approval_id, a.run_id, a.risk, a.expires_at, \
     u.id as author_user_id, u.email::text as author_email, u.display_name as author_name, \
     coalesce(t.title, m.title) as title, m.id is not null as of_mission, \
     ar.conversation_id, c.name as company \
     from approvals a \
     join agent_runs ar on ar.id = a.run_id \
     left join scheduled_tasks t on t.last_run_id = a.run_id \
       and t.user_id = a.requested_for_user_id \
     left join missions m on m.id = ar.mission_id \
       and m.owner_user_id = a.requested_for_user_id and m.status = 'in_progress' \
     join users u on u.id = a.requested_for_user_id \
     join tenants c on c.id = a.tenant_id \
     where a.run_id = $1 and (t.id is not null or m.id is not null) \
     and a.status = 'pending' and a.routing = 'requester' \
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

/// The requests of `run_id` its author was never told of (ADR-070), the
/// run paused by `lost_before` (a notice under way is sent well before,
/// ADR-086), not expired by `now`.
pub async fn author_notices_never_sent(
    conn: &mut PgConnection,
    run_id: Uuid,
    now: DateTime<Utc>,
    lost_before: DateTime<Utc>,
) -> sqlx::Result<Vec<AuthorNotice>> {
    sqlx::query_as(&format!(
        "{PENDING_FOR_AUTHOR} and (a.expires_at is null or a.expires_at > $3) \
         and ar.updated_at <= $4 order by a.created_at"
    ))
    .bind(run_id)
    .bind(NOTIFIED)
    .bind(now)
    .bind(lost_before)
    .fetch_all(conn)
    .await
}

/// Every company's scheduled runs with requests their author was never told
/// of, not expired by `now`, through a narrowly scoped database function:
/// each is then handled in its own company.
pub async fn unnotified_scheduled_runs(
    pool: &PgPool,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<NoticeRun>> {
    sqlx::query_as("select tenant_id, run_id from system_unnotified_scheduled_runs($1)")
        .bind(now)
        .fetch_all(pool)
        .await
}

/// Every company's mission stages with requests their author was never
/// told of, not expired by `now` (ADR-091), as the scheduled runs are.
pub async fn unnotified_mission_runs(
    pool: &PgPool,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<NoticeRun>> {
    sqlx::query_as("select tenant_id, run_id from system_unnotified_mission_runs($1)")
        .bind(now)
        .fetch_all(pool)
        .await
}
