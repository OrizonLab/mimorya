//! The agent's proposals of the company's skills (ADR-100): the skill a
//! proposal names, the proposals awaiting the administrators, and one they
//! turn down. The versions themselves are written by `skills`.

use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NamedSkillRow {
    pub id: Uuid,
    /// `draft`, `active` or `retired`.
    pub status: String,
}

/// The company's skill named `name`, if any.
pub async fn skill_named(
    conn: &mut PgConnection,
    name: &str,
) -> sqlx::Result<Option<NamedSkillRow>> {
    sqlx::query_as("select id, status from skills where name = $1")
        .bind(name)
        .fetch_optional(conn)
        .await
}

/// The company's proposals awaiting its administrators: all of them, and
/// those proposed for `person`.
pub async fn pending_proposals(conn: &mut PgConnection, person: Uuid) -> sqlx::Result<(i64, i64)> {
    sqlx::query_as(
        "select count(*), count(*) filter (where created_by_user_id = $1) \
         from skill_versions where proposed and status = 'candidate'",
    )
    .bind(person)
    .fetch_one(conn)
    .await
}

/// Turns down the proposal `version` of the skill; `false` when that
/// version is no proposal awaiting a decision.
pub async fn reject_proposal(
    conn: &mut PgConnection,
    skill_id: Uuid,
    version: i32,
) -> sqlx::Result<bool> {
    let rejected: Option<Uuid> = sqlx::query_scalar(
        "update skill_versions set status = 'rejected' \
         where skill_id = $1 and version = $2 and proposed and status = 'candidate' \
         returning id",
    )
    .bind(skill_id)
    .bind(version)
    .fetch_optional(&mut *conn)
    .await?;
    if rejected.is_none() {
        return Ok(false);
    }
    sqlx::query("update skills set updated_at = now() where id = $1")
        .bind(skill_id)
        .execute(conn)
        .await?;
    Ok(true)
}
