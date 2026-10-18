//! Skills (versioned procedures the agent can load).

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

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
}

pub async fn get_active_skill(
    conn: &mut PgConnection,
    name: &str,
) -> sqlx::Result<Option<ActiveSkill>> {
    sqlx::query_as(
        "select s.name, v.version, v.content from skills s \
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
    pub updated_at: DateTime<Utc>,
}

pub async fn list_skills(conn: &mut PgConnection) -> sqlx::Result<Vec<SkillRow>> {
    sqlx::query_as(
        "select s.id, s.name, s.description, s.status, v.version as active_version, s.updated_at \
         from skills s left join skill_versions v on v.id = s.active_version_id order by s.name",
    )
    .fetch_all(conn)
    .await
}
