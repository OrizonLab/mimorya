//! Agent runs: creation, state updates, listings and stale-run maintenance.

use chrono::{DateTime, Utc};
use mimorya_core::autonomy::AutonomyLevel;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub const ALL: [RunStatus; 6] = [
        Self::Queued,
        Self::Running,
        Self::AwaitingApproval,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }

    /// Where a run ends: a finished run is never rewritten, and its stream
    /// closes.
    pub const FINISHED: [RunStatus; 3] = [Self::Completed, Self::Failed, Self::Cancelled];

    pub fn is_finished(self) -> bool {
        Self::FINISHED.contains(&self)
    }

    /// The stored values of `FINISHED`, for queries.
    pub fn finished_values() -> Vec<&'static str> {
        Self::FINISHED.map(Self::as_str).to_vec()
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub initiated_by_user_id: Option<Uuid>,
    pub on_behalf_of_user_id: Uuid,
    pub status: String,
    pub objective: String,
    #[sqlx(json)]
    pub success_criteria: Vec<String>,
    pub autonomy_level: String,
    pub budget: Value,
    pub usage: Value,
    pub state: Option<Value>,
    pub result: Option<Value>,
    pub verification: Option<Value>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// The last event of the run's journal when its state was last saved
    /// (ADR-053).
    pub checkpoint_seq: Option<i32>,
    /// The turn's request (its user message), when started from a
    /// conversation (ADR-085).
    pub request_message_id: Option<Uuid>,
    /// The mission it is a stage of (ADR-089).
    pub mission_id: Option<Uuid>,
    /// The run that handed it its task, for a sub-agent's (ADR-098).
    pub parent_run_id: Option<Uuid>,
    /// `main`, or a sub-agent's profile (ADR-098).
    pub agent_profile: String,
    /// `web`, or `hook` when another system started it (ADR-109).
    pub channel: String,
}

impl RunRow {
    /// Unknown values (never written by the platform) read as `Failed`.
    pub fn status(&self) -> RunStatus {
        RunStatus::parse(&self.status).unwrap_or(RunStatus::Failed)
    }
}

pub(super) const RUN_COLUMNS: &str = "id, tenant_id, conversation_id, initiated_by_user_id, on_behalf_of_user_id, \
    status, objective, success_criteria, autonomy_level, budget, usage, state, result, verification, \
    error, started_at, finished_at, created_at, updated_at, checkpoint_seq, request_message_id, \
    mission_id, parent_run_id, agent_profile, channel";

#[derive(Debug, Clone)]
pub struct NewRun {
    pub tenant_id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub initiated_by_user_id: Uuid,
    pub on_behalf_of_user_id: Uuid,
    pub objective: String,
    pub success_criteria: Vec<String>,
    pub autonomy_level: AutonomyLevel,
    pub budget: Value,
    /// The turn's request (its user message), if any.
    pub request_message_id: Option<Uuid>,
    /// The mission it is a stage of, if any (ADR-089).
    pub mission_id: Option<Uuid>,
    /// `WEB_CHANNEL`, or `HOOK_CHANNEL` when another system starts it
    /// (ADR-109).
    pub channel: &'static str,
}

/// Creates a run in the `running` state.
pub async fn create_run(conn: &mut PgConnection, input: NewRun) -> sqlx::Result<RunRow> {
    sqlx::query_as(&format!(
        "insert into agent_runs (tenant_id, conversation_id, initiated_by_user_id, on_behalf_of_user_id, \
         objective, success_criteria, autonomy_level, budget, request_message_id, mission_id, \
         channel, status, started_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'running', now()) \
         returning {RUN_COLUMNS}"
    ))
    .bind(input.tenant_id)
    .bind(input.conversation_id)
    .bind(input.initiated_by_user_id)
    .bind(input.on_behalf_of_user_id)
    .bind(&input.objective)
    .bind(sqlx::types::Json(&input.success_criteria))
    .bind(input.autonomy_level.as_str())
    .bind(&input.budget)
    .bind(input.request_message_id)
    .bind(input.mission_id)
    .bind(input.channel)
    .fetch_one(conn)
    .await
}

pub async fn get_run(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<RunRow>> {
    sqlx::query_as(&format!(
        "select {RUN_COLUMNS} from agent_runs where id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Locks the run row until the end of the transaction (serialises decisions on a run).
pub async fn lock_run(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<RunRow>> {
    sqlx::query_as(&format!(
        "select {RUN_COLUMNS} from agent_runs where id = $1 for update"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Fields left to `None` are unchanged.
#[derive(Debug, Clone, Default)]
pub struct RunPatch {
    pub status: Option<RunStatus>,
    pub state: Option<Value>,
    pub usage: Option<Value>,
    pub result: Option<Value>,
    pub verification: Option<Value>,
    pub error: Option<String>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Saving a state also keeps the last event of the run's journal at that
/// moment (`checkpoint_seq`, ADR-053).
const UPDATE_RUN: &str = "update agent_runs set status = coalesce($2, status), \
    state = coalesce($3, state), \
    checkpoint_seq = case when $3::jsonb is null then checkpoint_seq \
      else (select coalesce(max(seq), 0) from run_events where run_id = $1) end, \
    usage = coalesce($4, usage), result = coalesce($5, result), \
    verification = coalesce($6, verification), error = coalesce($7, error), \
    finished_at = coalesce($8, finished_at), updated_at = now() where id = $1";

fn bind_patch<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    id: Uuid,
    patch: RunPatch,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    query
        .bind(id)
        .bind(patch.status.map(RunStatus::as_str))
        .bind(patch.state)
        .bind(patch.usage)
        .bind(patch.result)
        .bind(patch.verification)
        .bind(patch.error)
        .bind(patch.finished_at)
}

pub async fn update_run(conn: &mut PgConnection, id: Uuid, patch: RunPatch) -> sqlx::Result<()> {
    bind_patch(sqlx::query(UPDATE_RUN), id, patch)
        .execute(conn)
        .await?;
    Ok(())
}

/// Same as `update_run`, only while the run has not finished: a finished run
/// (completed, failed by the stale-run sweeper, cancelled) is never rewritten.
/// Returns whether the run was updated.
pub async fn update_active_run(
    conn: &mut PgConnection,
    id: Uuid,
    patch: RunPatch,
) -> sqlx::Result<bool> {
    let sql = format!("{UPDATE_RUN} and status <> all($9)");
    let result = bind_patch(sqlx::query(&sql), id, patch)
        .bind(RunStatus::finished_values())
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_runs_for_conversation(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<RunRow>> {
    sqlx::query_as(&format!(
        "select {RUN_COLUMNS} from agent_runs where conversation_id = $1 order by created_at desc limit $2"
    ))
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// The user's latest runs, those of sub-agents aside (ADR-098): each is
/// listed with the run that handed it its task.
pub async fn list_runs(
    conn: &mut PgConnection,
    on_behalf_of_user_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<RunRow>> {
    sqlx::query_as(&format!(
        "select {RUN_COLUMNS} from agent_runs where on_behalf_of_user_id = $1 \
         and parent_run_id is null order by created_at desc limit $2"
    ))
    .bind(on_behalf_of_user_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// A run the sweep failed, for its `run_finished` event.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StaleRun {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    pub error: String,
}

/// Marks runs left `running` without progress since `before` as failed
/// (stuck process), across tenants, through a narrowly scoped database
/// function, and returns them. Runs whose executor stopped are taken up by
/// their lease instead (`expired_run_leases`, ADR-086), which counts as
/// progress.
pub async fn fail_stale_runs(pool: &PgPool, before: DateTime<Utc>) -> sqlx::Result<Vec<StaleRun>> {
    sqlx::query_as("select tenant_id, run_id, error from system_fail_stale_runs($1)")
        .bind(before)
        .fetch_all(pool)
        .await
}

/// A run whose executor stopped (ADR-053, ADR-086): its lease expired, or
/// it never had one.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct InterruptedRun {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
}
