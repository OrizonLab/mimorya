//! Skills (versioned procedures the agent can load): what runs read, and
//! what the administration lists and writes (ADR-052). A version's content
//! never changes once written; activating one retires the version active
//! before it. The agent's proposals are written here too
//! (`skill_proposals`, ADR-100).

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// Every status of a skill (`skills_status_check`).
pub const SKILL_STATUSES: [&str; 3] = ["draft", "active", "retired"];
/// Every status of a version (`skill_versions_status_check`); `candidate`
/// and `rejected` are a proposal's, awaiting the administrators or turned
/// down by them (ADR-100).
pub const SKILL_VERSION_STATUSES: [&str; 5] =
    ["draft", "candidate", "active", "rejected", "retired"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
}

pub async fn list_active_skills(conn: &mut PgConnection) -> sqlx::Result<Vec<SkillSummary>> {
    sqlx::query_as("select name, description from skills where status = 'active' order by name")
        .fetch_all(conn)
        .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActiveSkill {
    pub name: String,
    pub version: i32,
    pub content: String,
    /// What an answer that follows it must meet (ADR-106).
    pub quality_criteria: Vec<String>,
}

pub async fn get_active_skill(
    conn: &mut PgConnection,
    name: &str,
) -> sqlx::Result<Option<ActiveSkill>> {
    sqlx::query_as(
        "select s.name, v.version, v.content, v.quality_criteria from skills s \
         join skill_versions v on v.id = s.active_version_id \
         where s.name = $1 and s.status = 'active'",
    )
    .bind(name)
    .fetch_optional(conn)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SkillRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
    pub active_version: Option<i32>,
    pub latest_version: Option<i32>,
    /// Its proposals awaiting the administrators (ADR-100).
    pub proposals: i64,
    pub updated_at: DateTime<Utc>,
}

const SKILL_SELECT: &str = "select s.id, s.name, s.description, s.status, \
     v.version as active_version, \
     (select max(version) from skill_versions where skill_id = s.id) as latest_version, \
     (select count(*) from skill_versions \
      where skill_id = s.id and proposed and status = 'candidate') as proposals, \
     s.updated_at \
     from skills s left join skill_versions v on v.id = s.active_version_id";

pub async fn list_skills(conn: &mut PgConnection) -> sqlx::Result<Vec<SkillRow>> {
    sqlx::query_as(&format!("{SKILL_SELECT} order by s.name"))
        .fetch_all(conn)
        .await
}

pub async fn get_skill(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<SkillRow>> {
    sqlx::query_as(&format!("{SKILL_SELECT} where s.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

pub async fn skill_name_taken(conn: &mut PgConnection, name: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("select exists (select 1 from skills where name = $1)")
        .bind(name)
        .fetch_one(conn)
        .await
}

pub async fn count_active_skills(conn: &mut PgConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar("select count(*) from skills where status = 'active'")
        .fetch_one(conn)
        .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SkillVersionRow {
    pub version: i32,
    pub status: String,
    pub content: String,
    pub changelog: String,
    /// What an answer that follows it must meet (ADR-106).
    pub quality_criteria: Vec<String>,
    /// Proposed by the agent for its author (ADR-100).
    pub proposed: bool,
    /// The description a proposal of a new skill gave (ADR-100).
    pub proposed_description: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
}

/// The versions of a skill, the latest first, with the names of their
/// author and of the person who last activated them.
pub async fn list_skill_versions(
    conn: &mut PgConnection,
    skill_id: Uuid,
) -> sqlx::Result<Vec<SkillVersionRow>> {
    sqlx::query_as(
        "select v.version, v.status, v.content, v.changelog, v.quality_criteria, v.proposed, \
         case when v.proposed then v.frontmatter->>'description' end as proposed_description, \
         author.display_name as created_by, \
         v.created_at, approver.display_name as approved_by, v.approved_at \
         from skill_versions v \
         left join users author on author.id = v.created_by_user_id \
         left join users approver on approver.id = v.approved_by_user_id \
         where v.skill_id = $1 order by v.version desc",
    )
    .bind(skill_id)
    .fetch_all(conn)
    .await
}

pub struct NewSkill<'a> {
    pub tenant_id: Uuid,
    pub name: &'a str,
    pub description: &'a str,
    pub created_by: Uuid,
}

/// A skill, as a draft without version.
pub async fn insert_skill(conn: &mut PgConnection, skill: NewSkill<'_>) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into skills (tenant_id, name, description, created_by_user_id) \
         values ($1, $2, $3, $4) returning id",
    )
    .bind(skill.tenant_id)
    .bind(skill.name)
    .bind(skill.description)
    .bind(skill.created_by)
    .fetch_one(conn)
    .await
}

pub struct NewSkillVersion<'a> {
    pub tenant_id: Uuid,
    pub skill_id: Uuid,
    pub content: &'a str,
    pub changelog: &'a str,
    /// What an answer that follows it must meet (ADR-106); when not given
    /// (a proposal of the agent's, a version written without them), those
    /// of the version active, none without one.
    pub quality_criteria: Option<&'a [String]>,
    pub created_by: Uuid,
    /// The run that proposed it for its author (ADR-100): a candidate then,
    /// a draft otherwise.
    pub proposed_by_run: Option<Uuid>,
    /// The description a proposal of a new skill gave, kept in its
    /// frontmatter as agentskills.io writes it.
    pub description: Option<&'a str>,
}

/// A version numbered after the latest one; its number.
pub async fn insert_skill_version(
    conn: &mut PgConnection,
    version: NewSkillVersion<'_>,
) -> sqlx::Result<i32> {
    let number: i32 = sqlx::query_scalar(
        "insert into skill_versions (tenant_id, skill_id, version, content, frontmatter, \
         changelog, created_by_user_id, created_by_run_id, proposed, status, quality_criteria) \
         select $1, $2, coalesce(max(version), 0) + 1, $3, \
         jsonb_strip_nulls(jsonb_build_object('name', (select name from skills where id = $2), \
         'description', $7::text)), $4, $5, $6, \
         $6::uuid is not null, case when $6::uuid is null then 'draft' else 'candidate' end, \
         coalesce($8, (select active.quality_criteria from skills s \
         join skill_versions active on active.id = s.active_version_id where s.id = $2), '{}') \
         from skill_versions where skill_id = $2 returning version",
    )
    .bind(version.tenant_id)
    .bind(version.skill_id)
    .bind(version.content)
    .bind(version.changelog)
    .bind(version.created_by)
    .bind(version.proposed_by_run)
    .bind(version.description)
    .bind(version.quality_criteria)
    .fetch_one(&mut *conn)
    .await?;
    touch(conn, version.skill_id).await?;
    Ok(number)
}

/// Makes `version` the active one, approved by `approved_by`; the version
/// active before is retired. `false` when the skill has no such version.
pub async fn activate_skill_version(
    conn: &mut PgConnection,
    skill_id: Uuid,
    version: i32,
    approved_by: Uuid,
) -> sqlx::Result<bool> {
    let activated: Option<Uuid> = sqlx::query_scalar(
        "update skill_versions set status = 'active', approved_by_user_id = $3, approved_at = now() \
         where skill_id = $1 and version = $2 returning id",
    )
    .bind(skill_id)
    .bind(version)
    .bind(approved_by)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(version_id) = activated else {
        return Ok(false);
    };
    sqlx::query(
        "update skill_versions set status = 'retired' \
         where skill_id = $1 and status = 'active' and id <> $2",
    )
    .bind(skill_id)
    .bind(version_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "update skills set status = 'active', active_version_id = $2, updated_at = now() \
         where id = $1",
    )
    .bind(skill_id)
    .bind(version_id)
    .execute(conn)
    .await?;
    Ok(true)
}

/// No version of the skill is active any more: runs no longer see it.
pub async fn retire_skill(conn: &mut PgConnection, skill_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "update skill_versions set status = 'retired' where skill_id = $1 and status = 'active'",
    )
    .bind(skill_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "update skills set status = 'retired', active_version_id = null, updated_at = now() \
         where id = $1",
    )
    .bind(skill_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update_skill_description(
    conn: &mut PgConnection,
    skill_id: Uuid,
    description: &str,
) -> sqlx::Result<()> {
    sqlx::query("update skills set description = $2, updated_at = now() where id = $1")
        .bind(skill_id)
        .bind(description)
        .execute(conn)
        .await?;
    Ok(())
}

/// The skill and its versions (evaluations go with them).
pub async fn delete_skill(conn: &mut PgConnection, skill_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("delete from skills where id = $1")
        .bind(skill_id)
        .execute(conn)
        .await?;
    Ok(())
}

async fn touch(conn: &mut PgConnection, skill_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("update skills set updated_at = now() where id = $1")
        .bind(skill_id)
        .execute(conn)
        .await?;
    Ok(())
}
