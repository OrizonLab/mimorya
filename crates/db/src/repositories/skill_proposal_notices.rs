//! What tells the skills' administrators of the agent's proposals
//! (ADR-102): the companies holding proposals never announced, those
//! proposals, and the record that they were told, or that the relay refused
//! it. The accounts that may decide them: `administrators.rs`.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A proposal awaiting a decision, never announced.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UnannouncedProposal {
    pub version_id: Uuid,
    /// The skill it is a version of.
    pub name: String,
}

/// Every active company holding proposals never announced, through a
/// narrowly scoped database function: each is then read in its own
/// company.
pub async fn companies_with_unannounced_proposals(pool: &PgPool) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("select tenant_id from system_unannounced_skill_proposals()")
        .fetch_all(pool)
        .await
}

/// The company's proposals awaiting a decision and never announced, by
/// skill name then version.
pub async fn unannounced_proposals(
    conn: &mut PgConnection,
) -> sqlx::Result<Vec<UnannouncedProposal>> {
    sqlx::query_as(
        "select v.id as version_id, s.name from skill_versions v \
         join skills s on s.id = v.skill_id \
         where v.proposed and v.status = 'candidate' and v.announced_at is null \
         order by s.name, v.version",
    )
    .fetch_all(conn)
    .await
}

/// Records that `version_ids` were announced, or that the relay refused
/// their notice: none is announced again.
pub async fn record_announced(conn: &mut PgConnection, version_ids: &[Uuid]) -> sqlx::Result<()> {
    sqlx::query(
        "update skill_versions set announced_at = now() \
         where id = any($1) and announced_at is null",
    )
    .bind(version_ids)
    .execute(conn)
    .await?;
    Ok(())
}
