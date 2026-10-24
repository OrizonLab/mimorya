//! What tells a mission's author that it is over (ADR-091): done, or
//! stopped unfinished, whichever path ended it, but not when they stopped it
//! themselves. Told once, by the periodic pass, within a while of its end:
//! the missions over since then, never told of, their company and their
//! author's account active.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A mission over in some company, to tell its author of.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct EndedMission {
    pub tenant_id: Uuid,
    pub mission_id: Uuid,
}

/// What the author is told of, and where.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MissionEndNotice {
    pub mission_id: Uuid,
    pub title: String,
    /// `done`, or `failed` (a mission under way whose last stage failed
    /// reads `in_progress` here).
    pub status: String,
    /// The review of its final answer, when it is done.
    pub verification: Option<Value>,
    pub owner_user_id: Uuid,
    pub owner_email: String,
    pub owner_name: String,
    pub company: String,
}

/// Every company's missions over since `since`, never told to their author,
/// through a narrowly scoped database function: each is then told of in its
/// own company.
pub async fn unnotified_mission_ends(
    pool: &PgPool,
    since: DateTime<Utc>,
) -> sqlx::Result<Vec<EndedMission>> {
    sqlx::query_as("select tenant_id, mission_id from system_unnotified_mission_ends($1)")
        .bind(since)
        .fetch_all(pool)
        .await
}

/// The mission `id`, if it is over since `since` and still not told of,
/// its company and its author's account active.
pub async fn mission_end_notice(
    conn: &mut PgConnection,
    id: Uuid,
    since: DateTime<Utc>,
) -> sqlx::Result<Option<MissionEndNotice>> {
    sqlx::query_as(
        "select m.id as mission_id, m.title, m.status, m.verification, \
         u.id as owner_user_id, u.email::text as owner_email, u.display_name as owner_name, \
         c.name as company \
         from missions m \
         join users u on u.id = m.owner_user_id \
         join tenants c on c.id = m.tenant_id \
         left join lateral (select r.status, r.finished_at from agent_runs r \
           where r.mission_id = m.id order by r.created_at desc, r.id desc limit 1) s on true \
         where m.id = $1 and m.end_notified_at is null and m.status <> 'cancelled' \
         and c.status = 'active' and u.status = 'active' \
         and ((m.status in ('done', 'failed') and m.finished_at > $2) \
           or (m.status = 'in_progress' and s.status in ('failed', 'cancelled') \
             and s.finished_at > $2))",
    )
    .bind(id)
    .bind(since)
    .fetch_optional(conn)
    .await
}

/// Records that the mission's author was told of its end, or that the
/// relay refused the message: it is not sent again.
pub async fn record_end_notice(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "update missions set end_notified_at = now() where id = $1 and end_notified_at is null",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(())
}
