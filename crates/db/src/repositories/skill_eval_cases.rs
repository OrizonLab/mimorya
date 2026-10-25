//! The evaluation cases of the company's skills (ADR-103): written by the
//! skills' administrators, read to try a version before it is made active.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvalCaseRow {
    pub id: Uuid,
    pub skill_id: Uuid,
    pub name: String,
    pub request: String,
    pub criteria: Vec<String>,
    /// Its author's name, while the account exists.
    pub created_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

const CASE_SELECT: &str = "select c.id, c.skill_id, c.name, c.request, c.criteria, \
     author.display_name as created_by, c.updated_at \
     from skill_eval_cases c left join users author on author.id = c.created_by_user_id";

/// The cases of a skill, in the order they were written.
pub async fn list_eval_cases(
    conn: &mut PgConnection,
    skill_id: Uuid,
) -> sqlx::Result<Vec<EvalCaseRow>> {
    sqlx::query_as(&format!(
        "{CASE_SELECT} where c.skill_id = $1 order by c.created_at, c.id"
    ))
    .bind(skill_id)
    .fetch_all(conn)
    .await
}

pub async fn get_eval_case(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<EvalCaseRow>> {
    sqlx::query_as(&format!("{CASE_SELECT} where c.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

pub async fn count_eval_cases(conn: &mut PgConnection, skill_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("select count(*) from skill_eval_cases where skill_id = $1")
        .bind(skill_id)
        .fetch_one(conn)
        .await
}

/// Whether another case of the skill than `except` is named `name`.
pub async fn eval_case_name_taken(
    conn: &mut PgConnection,
    skill_id: Uuid,
    name: &str,
    except: Option<Uuid>,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select exists (select 1 from skill_eval_cases \
         where skill_id = $1 and name = $2 and id is distinct from $3)",
    )
    .bind(skill_id)
    .bind(name)
    .bind(except)
    .fetch_one(conn)
    .await
}

pub struct EvalCaseText<'a> {
    pub name: &'a str,
    pub request: &'a str,
    pub criteria: &'a [String],
}

pub async fn insert_eval_case(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    skill_id: Uuid,
    case: EvalCaseText<'_>,
    created_by: Uuid,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into skill_eval_cases \
         (tenant_id, skill_id, name, request, criteria, created_by_user_id) \
         values ($1, $2, $3, $4, $5, $6) returning id",
    )
    .bind(tenant_id)
    .bind(skill_id)
    .bind(case.name)
    .bind(case.request)
    .bind(case.criteria)
    .bind(created_by)
    .fetch_one(conn)
    .await
}

pub async fn update_eval_case(
    conn: &mut PgConnection,
    id: Uuid,
    case: EvalCaseText<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update skill_eval_cases set name = $2, request = $3, criteria = $4, updated_at = now() \
         where id = $1",
    )
    .bind(id)
    .bind(case.name)
    .bind(case.request)
    .bind(case.criteria)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_eval_case(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    sqlx::query("delete from skill_eval_cases where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}
