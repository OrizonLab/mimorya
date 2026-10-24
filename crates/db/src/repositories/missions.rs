//! Missions (ADR-089): work a person entrusts to their agent, carried out in
//! stages, each a run of the mission's conversation. Read and stopped by
//! their owner; carried on, or ended, by the platform as each stage ends,
//! the mission locked meanwhile. A mission still in progress reads as its
//! last stage stands: awaiting a decision, or failed elsewhere.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::runs::{RUN_COLUMNS, RunRow};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MissionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub owner_user_id: Uuid,
    pub conversation_id: Uuid,
    pub title: String,
    /// What it is to achieve: each stage's objective.
    pub description: String,
    #[sqlx(json)]
    pub success_criteria: Vec<String>,
    pub autonomy_level: String,
    /// `in_progress`, `done`, `failed` or `cancelled`.
    pub status: String,
    /// Its own limits, as when it was entrusted.
    pub limits: Value,
    /// Stages started.
    pub stages: i32,
    /// The sources its stages collected.
    pub evidence: Value,
    pub result: Option<String>,
    pub verification: Option<Value>,
    pub error: Option<String>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const COLUMNS: &str = "m.id, m.tenant_id, m.owner_user_id, m.conversation_id, m.title, \
     m.description, m.success_criteria, m.autonomy_level, m.status, m.limits, m.stages, \
     m.evidence, m.result, m.verification, m.error, m.finished_at, m.created_at, m.updated_at";

/// A mission with its last stage and what its stages cost.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MissionView {
    #[sqlx(flatten)]
    pub mission: MissionRow,
    pub stage_run_id: Option<Uuid>,
    pub stage_status: Option<String>,
    pub stage_error: Option<String>,
    pub cost_micros: i64,
}

const VIEW: &str = "select {COLUMNS}, s.id as stage_run_id, s.status as stage_status, \
     s.error as stage_error, \
     (select coalesce(sum((r.usage->>'costMicros')::bigint), 0) from agent_runs r \
       where r.mission_id = m.id)::bigint as cost_micros \
     from missions m \
     left join lateral (select r.id, r.status, r.error from agent_runs r \
       where r.mission_id = m.id order by r.created_at desc, r.id desc limit 1) s on true";

fn view_sql(filter: &str) -> String {
    format!("{} {filter}", VIEW.replace("{COLUMNS}", COLUMNS))
}

pub struct NewMission<'a> {
    pub tenant_id: Uuid,
    pub owner_user_id: Uuid,
    pub conversation_id: Uuid,
    pub title: &'a str,
    pub description: &'a str,
    pub success_criteria: &'a [String],
    pub autonomy_level: &'a str,
    pub limits: Value,
}

/// A mission in progress, at its first stage.
pub async fn create_mission(
    conn: &mut PgConnection,
    input: NewMission<'_>,
) -> sqlx::Result<MissionRow> {
    sqlx::query_as(&format!(
        "insert into missions as m (tenant_id, owner_user_id, conversation_id, title, \
         description, success_criteria, autonomy_level, status, limits) \
         values ($1, $2, $3, $4, $5, $6, $7, 'in_progress', $8) returning {COLUMNS}"
    ))
    .bind(input.tenant_id)
    .bind(input.owner_user_id)
    .bind(input.conversation_id)
    .bind(input.title)
    .bind(input.description)
    .bind(sqlx::types::Json(input.success_criteria))
    .bind(input.autonomy_level)
    .bind(&input.limits)
    .fetch_one(conn)
    .await
}

/// The missions of `owner_user_id`, newest first.
pub async fn list_missions(
    conn: &mut PgConnection,
    owner_user_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<MissionView>> {
    sqlx::query_as(&view_sql(
        "where m.owner_user_id = $1 order by m.created_at desc, m.id desc limit $2",
    ))
    .bind(owner_user_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// The mission `id` of `owner_user_id`.
pub async fn get_mission(
    conn: &mut PgConnection,
    id: Uuid,
    owner_user_id: Uuid,
) -> sqlx::Result<Option<MissionView>> {
    sqlx::query_as(&view_sql("where m.id = $1 and m.owner_user_id = $2"))
        .bind(id)
        .bind(owner_user_id)
        .fetch_optional(conn)
        .await
}

/// The mission `id`, locked against another change until the transaction
/// ends.
pub async fn lock_mission(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<MissionRow>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from missions m where m.id = $1 for update"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// The mission's last stage, if it is still under way (queued, running or
/// awaiting a decision).
pub async fn active_stage(
    conn: &mut PgConnection,
    mission_id: Uuid,
) -> sqlx::Result<Option<RunRow>> {
    sqlx::query_as(&format!(
        "select {RUN_COLUMNS} from agent_runs where mission_id = $1 \
         and status in ('queued', 'running', 'awaiting_approval') \
         order by created_at desc, id desc limit 1"
    ))
    .bind(mission_id)
    .fetch_optional(conn)
    .await
}

/// How many missions of `owner_user_id` have a stage under way, under a lock
/// of theirs until the transaction ends: two missions entrusted at once
/// count each other.
pub async fn count_missions_under_way(
    conn: &mut PgConnection,
    owner_user_id: Uuid,
) -> sqlx::Result<i64> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 89))")
        .bind(owner_user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query_scalar(
        "select count(*) from missions m where m.owner_user_id = $1 \
         and m.status = 'in_progress' and exists (select 1 from agent_runs r \
           where r.mission_id = m.id and r.status in ('queued', 'running', 'awaiting_approval'))",
    )
    .bind(owner_user_id)
    .fetch_one(conn)
    .await
}

/// One more stage started, with the sources the stages collected so far.
pub async fn record_stage(conn: &mut PgConnection, id: Uuid, evidence: &Value) -> sqlx::Result<()> {
    sqlx::query(
        "update missions set stages = stages + 1, evidence = $2, updated_at = now() \
         where id = $1",
    )
    .bind(id)
    .bind(evidence)
    .execute(conn)
    .await?;
    Ok(())
}

/// How a mission ended.
pub struct MissionEnd<'a> {
    /// `done`, `failed` or `cancelled`.
    pub status: &'a str,
    pub result: Option<&'a str>,
    pub verification: Option<Value>,
    pub error: Option<&'a str>,
}

/// Ends the mission, if still in progress; whether it was.
pub async fn end_mission(
    conn: &mut PgConnection,
    id: Uuid,
    end: MissionEnd<'_>,
) -> sqlx::Result<bool> {
    let ended = sqlx::query(
        "update missions set status = $2, result = $3, verification = $4, error = $5, \
         finished_at = now(), updated_at = now() where id = $1 and status = 'in_progress'",
    )
    .bind(id)
    .bind(end.status)
    .bind(end.result)
    .bind(end.verification)
    .bind(end.error)
    .execute(conn)
    .await?;
    Ok(ended.rows_affected() == 1)
}

/// What the mission's stages cost so far.
pub async fn mission_cost(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select coalesce(sum((usage->>'costMicros')::bigint), 0)::bigint from agent_runs \
         where mission_id = $1",
    )
    .bind(id)
    .fetch_one(conn)
    .await
}

/// The sources the mission's stages collected so far.
pub async fn mission_evidence(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<Value>> {
    sqlx::query_scalar("select evidence from missions where id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await
}
