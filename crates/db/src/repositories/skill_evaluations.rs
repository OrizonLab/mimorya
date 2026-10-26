//! Evaluations of a skill's versions (ADR-104): asked for by an
//! administrator, played by a process that executes runs, case by case and
//! version by version, each result written once; the last ones of each
//! skill kept. Which ones a process has to play crosses the companies'
//! boundary through `system_pending_skill_evaluations` only.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvaluationRow {
    pub id: Uuid,
    pub skill_id: Uuid,
    pub version: i32,
    pub reference_version: Option<i32>,
    pub status: String,
    pub failure: Option<String>,
    /// The cases as they stood when it was asked for.
    pub cases: Value,
    pub launched_by_user_id: Option<Uuid>,
    /// Its author's name, while the account exists.
    pub launched_by: Option<String>,
    pub cost_micros: i64,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

const EVALUATION_SELECT: &str = "select e.id, e.skill_id, e.version, e.reference_version, \
     e.status, e.failure, e.cases, e.launched_by_user_id, author.display_name as launched_by, \
     e.cost_micros, e.created_at, e.started_at, e.finished_at \
     from skill_evaluations e left join users author on author.id = e.launched_by_user_id";

pub struct NewEvaluation<'a> {
    pub tenant_id: Uuid,
    pub skill_id: Uuid,
    pub version: i32,
    pub reference_version: Option<i32>,
    pub launched_by: Uuid,
    pub cases: &'a Value,
}

pub async fn insert_evaluation(
    conn: &mut PgConnection,
    evaluation: NewEvaluation<'_>,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into skill_evaluations \
         (tenant_id, skill_id, version, reference_version, launched_by_user_id, cases) \
         values ($1, $2, $3, $4, $5, $6) returning id",
    )
    .bind(evaluation.tenant_id)
    .bind(evaluation.skill_id)
    .bind(evaluation.version)
    .bind(evaluation.reference_version)
    .bind(evaluation.launched_by)
    .bind(evaluation.cases)
    .fetch_one(conn)
    .await
}

/// Whether the company has an evaluation queued or running.
pub async fn evaluation_in_progress(conn: &mut PgConnection) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select exists (select 1 from skill_evaluations where status in ('queued', 'running'))",
    )
    .fetch_one(conn)
    .await
}

/// Deletes the skill's evaluations past its `keep` latest, results with
/// them; one queued or running is never deleted.
pub async fn prune_evaluations(
    conn: &mut PgConnection,
    skill_id: Uuid,
    keep: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "delete from skill_evaluations where skill_id = $1 \
         and status in ('completed', 'failed') \
         and id not in (select id from skill_evaluations where skill_id = $1 \
                        order by created_at desc, id desc limit $2)",
    )
    .bind(skill_id)
    .bind(keep)
    .execute(conn)
    .await?;
    Ok(())
}

/// The skill's evaluations, the latest first.
pub async fn list_evaluations(
    conn: &mut PgConnection,
    skill_id: Uuid,
) -> sqlx::Result<Vec<EvaluationRow>> {
    sqlx::query_as(&format!(
        "{EVALUATION_SELECT} where e.skill_id = $1 order by e.created_at desc, e.id desc"
    ))
    .bind(skill_id)
    .fetch_all(conn)
    .await
}

pub async fn get_evaluation(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<EvaluationRow>> {
    sqlx::query_as(&format!("{EVALUATION_SELECT} where e.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// The last completed evaluation of the skill's `version`.
pub async fn last_completed_evaluation(
    conn: &mut PgConnection,
    skill_id: Uuid,
    version: i32,
) -> sqlx::Result<Option<EvaluationRow>> {
    sqlx::query_as(&format!(
        "{EVALUATION_SELECT} where e.skill_id = $1 and e.version = $2 \
         and e.status = 'completed' order by e.created_at desc, e.id desc limit 1"
    ))
    .bind(skill_id)
    .bind(version)
    .fetch_optional(conn)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ResultRow {
    pub evaluation_id: Uuid,
    pub position: i32,
    pub reference: bool,
    pub outcome: String,
    pub issues: Vec<String>,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub skill_loaded: bool,
    pub tools: Value,
    pub cost_micros: i64,
}

/// The results of the evaluations `ids`, case by case.
pub async fn results_of(conn: &mut PgConnection, ids: &[Uuid]) -> sqlx::Result<Vec<ResultRow>> {
    sqlx::query_as(
        "select evaluation_id, position, reference, outcome, issues, answer, error, \
         skill_loaded, tools, cost_micros from skill_evaluation_results \
         where evaluation_id = any($1) order by evaluation_id, position, reference",
    )
    .bind(ids)
    .fetch_all(conn)
    .await
}

/// An evaluation a process has to play, or one a stopped process left
/// running.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct PendingEvaluation {
    pub tenant_id: Uuid,
    pub id: Uuid,
    pub running: bool,
}

pub async fn pending_evaluations(pool: &PgPool) -> sqlx::Result<Vec<PendingEvaluation>> {
    sqlx::query_as("select tenant_id, id, running from system_pending_skill_evaluations()")
        .fetch_all(pool)
        .await
}

/// Starts the queued evaluation `id`: `None` when it is not queued any more.
pub async fn claim_evaluation(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<EvaluationRow>> {
    let claimed: Option<Uuid> = sqlx::query_scalar(
        "update skill_evaluations set status = 'running', started_at = now() \
         where id = $1 and status = 'queued' returning id",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    match claimed {
        Some(id) => get_evaluation(conn, id).await,
        None => Ok(None),
    }
}

pub struct NewResult<'a> {
    pub tenant_id: Uuid,
    pub evaluation_id: Uuid,
    pub position: i32,
    pub reference: bool,
    pub outcome: &'a str,
    pub issues: &'a [String],
    pub answer: Option<&'a str>,
    pub error: Option<&'a str>,
    pub skill_loaded: bool,
    pub tools: &'a Value,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micros: i64,
}

/// Writes one case's result and counts what it cost in its evaluation,
/// while it is running; `false` when it is not any more (failed as
/// interrupted meanwhile): nothing written.
pub async fn record_result(conn: &mut PgConnection, result: NewResult<'_>) -> sqlx::Result<bool> {
    let written = sqlx::query(
        "insert into skill_evaluation_results (tenant_id, evaluation_id, position, reference, \
         outcome, issues, answer, error, skill_loaded, tools, cost_micros) \
         select $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11 \
         where exists (select 1 from skill_evaluations where id = $2 and status = 'running')",
    )
    .bind(result.tenant_id)
    .bind(result.evaluation_id)
    .bind(result.position)
    .bind(result.reference)
    .bind(result.outcome)
    .bind(result.issues)
    .bind(result.answer)
    .bind(result.error)
    .bind(result.skill_loaded)
    .bind(result.tools)
    .bind(result.cost_micros)
    .execute(&mut *conn)
    .await?;
    if written.rows_affected() == 0 {
        return Ok(false);
    }
    sqlx::query(
        "update skill_evaluations set input_tokens = input_tokens + $2, \
         output_tokens = output_tokens + $3, cost_micros = cost_micros + $4 where id = $1",
    )
    .bind(result.evaluation_id)
    .bind(result.input_tokens)
    .bind(result.output_tokens)
    .bind(result.cost_micros)
    .execute(conn)
    .await?;
    Ok(true)
}

/// Deletes the evaluations of every company finished before `before`,
/// results with them; how many.
pub async fn purge_finished_evaluations(pool: &PgPool, before: DateTime<Utc>) -> sqlx::Result<i32> {
    sqlx::query_scalar("select system_purge_skill_evaluations($1)")
        .bind(before)
        .fetch_one(pool)
        .await
}

/// Ends the running evaluation `id`, failed for `failure` when given;
/// `false` when it was not running.
pub async fn finish_evaluation(
    conn: &mut PgConnection,
    id: Uuid,
    failure: Option<&str>,
) -> sqlx::Result<bool> {
    let ended = sqlx::query(
        "update skill_evaluations set status = $2, failure = $3, finished_at = now() \
         where id = $1 and status = 'running'",
    )
    .bind(id)
    .bind(if failure.is_some() {
        "failed"
    } else {
        "completed"
    })
    .bind(failure)
    .execute(conn)
    .await?;
    Ok(ended.rows_affected() == 1)
}

/// What a run evaluating a version reads of it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VersionText {
    pub name: String,
    pub description: String,
    pub content: String,
    /// What an answer that follows it must meet (ADR-106).
    pub quality_criteria: Vec<String>,
}

/// The skill's `version`, as the index and `skill_read` give it.
pub async fn version_text(
    conn: &mut PgConnection,
    skill_id: Uuid,
    version: i32,
) -> sqlx::Result<Option<VersionText>> {
    sqlx::query_as(
        "select s.name, s.description, v.content, v.quality_criteria from skills s \
         join skill_versions v on v.skill_id = s.id \
         where s.id = $1 and v.version = $2",
    )
    .bind(skill_id)
    .bind(version)
    .fetch_optional(conn)
    .await
}
