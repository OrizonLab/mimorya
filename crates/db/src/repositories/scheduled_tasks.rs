//! Requests a person asks their agent to make on a schedule (ADR-080): read
//! and written by their author only; fired when due, one at a time (the
//! row locked), with what became of the last occurrence.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScheduledTaskRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub instruction: String,
    pub weekdays: Vec<i16>,
    pub time_of_day: String,
    pub time_zone: String,
    /// When it runs next; `None` while paused.
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
    pub last_problem: Option<String>,
    /// The conversation of the last run an occurrence started.
    pub last_conversation_id: Option<Uuid>,
    /// The last run an occurrence started: until it ends, none starts.
    pub last_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const COLUMNS: &str = "t.id, t.user_id, t.title, t.instruction, t.weekdays, t.time_of_day, \
     t.time_zone, t.next_run_at, t.last_run_at, t.last_outcome, t.last_problem, \
     t.last_conversation_id, t.last_run_id, t.created_at, t.updated_at";

/// What a task asks and when: written whole, at creation or on a change.
pub struct TaskDefinition<'a> {
    pub title: &'a str,
    pub instruction: &'a str,
    pub weekdays: &'a [i16],
    pub time_of_day: &'a str,
    pub time_zone: &'a str,
    pub next_run_at: Option<DateTime<Utc>>,
}

/// The tasks of `user_id`, oldest first.
pub async fn list_scheduled_tasks(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ScheduledTaskRow>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from scheduled_tasks t where t.user_id = $1 order by t.created_at, t.id"
    ))
    .bind(user_id)
    .fetch_all(conn)
    .await
}

/// The task `id` of `user_id`, locked against another change.
pub async fn lock_scheduled_task(
    conn: &mut PgConnection,
    id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<ScheduledTaskRow>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from scheduled_tasks t where t.id = $1 and t.user_id = $2 for update"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// How many tasks `user_id` has, under a lock of theirs until the
/// transaction ends: two creations at once count each other.
pub async fn count_scheduled_tasks(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 80))")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query_scalar("select count(*) from scheduled_tasks where user_id = $1")
        .bind(user_id)
        .fetch_one(conn)
        .await
}

pub async fn create_scheduled_task(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    task: TaskDefinition<'_>,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into scheduled_tasks (tenant_id, user_id, title, instruction, weekdays, \
         time_of_day, time_zone, next_run_at) values ($1, $2, $3, $4, $5, $6, $7, $8) \
         returning id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(task.title)
    .bind(task.instruction)
    .bind(task.weekdays)
    .bind(task.time_of_day)
    .bind(task.time_zone)
    .bind(task.next_run_at)
    .fetch_one(conn)
    .await
}

/// Rewrites the task `id` (locked first by `lock_scheduled_task`).
pub async fn update_scheduled_task(
    conn: &mut PgConnection,
    id: Uuid,
    task: TaskDefinition<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update scheduled_tasks set title = $2, instruction = $3, weekdays = $4, \
         time_of_day = $5, time_zone = $6, next_run_at = $7, updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(task.title)
    .bind(task.instruction)
    .bind(task.weekdays)
    .bind(task.time_of_day)
    .bind(task.time_zone)
    .bind(task.next_run_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the task `id` of `user_id`; whether there was one.
pub async fn delete_scheduled_task(
    conn: &mut PgConnection,
    id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let deleted = sqlx::query("delete from scheduled_tasks where id = $1 and user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// A task due, locked, with what its author's account allows now.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DueTask {
    #[sqlx(flatten)]
    pub task: ScheduledTaskRow,
    pub owner_status: String,
    pub must_change_password: bool,
    /// The status of the last run an occurrence started, if any.
    pub last_run_status: Option<String>,
}

/// The task `id` if due by `now`; `None` when another process fires it, or
/// it changed meanwhile.
pub async fn lock_due_task(
    conn: &mut PgConnection,
    id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<Option<DueTask>> {
    sqlx::query_as(&format!(
        "select {COLUMNS}, u.status as owner_status, u.must_change_password, \
         r.status as last_run_status from scheduled_tasks t \
         join users u on u.id = t.user_id \
         left join agent_runs r on r.id = t.last_run_id \
         where t.id = $1 and t.next_run_at <= $2 for update of t skip locked"
    ))
    .bind(id)
    .bind(now)
    .fetch_optional(conn)
    .await
}

/// What became of an occurrence.
pub struct Occurrence<'a> {
    pub at: DateTime<Utc>,
    pub outcome: &'a str,
    pub problem: Option<&'a str>,
    pub conversation_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
}

/// Sets when the task `id` runs next: claimed, an occurrence never fires
/// twice, whatever follows.
pub async fn reschedule_task(
    conn: &mut PgConnection,
    id: Uuid,
    next_run_at: Option<DateTime<Utc>>,
) -> sqlx::Result<()> {
    sqlx::query("update scheduled_tasks set next_run_at = $2 where id = $1")
        .bind(id)
        .bind(next_run_at)
        .execute(conn)
        .await?;
    Ok(())
}

/// Makes `run_id`, in `conversation_id`, the last run an occurrence of the
/// task `id` started, in the transaction that opens it: before it can run,
/// and so pause, the next occurrence waits for it and its author is told of
/// its requests (ADR-082).
pub async fn link_started_run(
    conn: &mut PgConnection,
    id: Uuid,
    conversation_id: Uuid,
    run_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "update scheduled_tasks set last_conversation_id = $2, last_run_id = $3 where id = $1",
    )
    .bind(id)
    .bind(conversation_id)
    .bind(run_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Records what became of an occurrence of the task `id`, leaving when it
/// runs next as it is (its author may have changed it meanwhile). One that
/// started no run keeps the last run started, and its conversation: the
/// next occurrence waits for that run still.
pub async fn record_occurrence(
    conn: &mut PgConnection,
    id: Uuid,
    occurrence: Occurrence<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update scheduled_tasks set last_run_at = $2, last_outcome = $3, last_problem = $4, \
         last_conversation_id = coalesce($5, last_conversation_id), \
         last_run_id = coalesce($6, last_run_id) where id = $1",
    )
    .bind(id)
    .bind(occurrence.at)
    .bind(occurrence.outcome)
    .bind(occurrence.problem)
    .bind(occurrence.conversation_id)
    .bind(occurrence.run_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// The tasks of every company due by `due_by`, soonest first, at most
/// `batch`: company and identifier.
pub async fn due_scheduled_tasks(
    pool: &PgPool,
    due_by: DateTime<Utc>,
    batch: i32,
) -> sqlx::Result<Vec<(Uuid, Uuid)>> {
    sqlx::query_as("select tenant_id, task_id from system_due_scheduled_tasks($1, $2)")
        .bind(due_by)
        .bind(batch)
        .fetch_all(pool)
        .await
}
