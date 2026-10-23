//! The run queue (ADR-085): a run an `api` process sets going waits
//! `queued` for a worker, which claims it in its company, skipping the runs
//! another worker is claiming (`FOR UPDATE SKIP LOCKED`), then executes it.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::runs::{InterruptedRun, RUN_COLUMNS, RunRow};

/// Queues `run_id`, just set running, for a worker; whether it was.
pub async fn queue_run(conn: &mut PgConnection, run_id: Uuid) -> sqlx::Result<bool> {
    let queued = sqlx::query(
        "update agent_runs set status = 'queued', updated_at = now() \
         where id = $1 and status = 'running'",
    )
    .bind(run_id)
    .execute(conn)
    .await?;
    Ok(queued.rows_affected() == 1)
}

/// A run claimed from the queue, and since when it waited.
#[derive(Debug, Clone)]
pub struct ClaimedRun {
    pub run: RunRow,
    pub queued_since: DateTime<Utc>,
}

/// Claims `run_id` from the queue, setting it running; `None` when it is no
/// longer queued, or being claimed by another worker.
pub async fn claim_queued_run(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> sqlx::Result<Option<ClaimedRun>> {
    let queued: Option<(DateTime<Utc>,)> = sqlx::query_as(
        "select updated_at from agent_runs where id = $1 and status = 'queued' \
         for update skip locked",
    )
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((queued_since,)) = queued else {
        return Ok(None);
    };
    let run: RunRow = sqlx::query_as(&format!(
        "update agent_runs set status = 'running', updated_at = now() where id = $1 \
         returning {RUN_COLUMNS}"
    ))
    .bind(run_id)
    .fetch_one(conn)
    .await?;
    Ok(Some(ClaimedRun { run, queued_since }))
}

/// Every company's queued runs, the longest waiting first, `limit` at most,
/// through a narrowly scoped database function: each is then claimed in its
/// own company.
pub async fn queued_runs(pool: &PgPool, limit: i32) -> sqlx::Result<Vec<InterruptedRun>> {
    sqlx::query_as("select tenant_id, run_id from system_queued_runs($1)")
        .bind(limit)
        .fetch_all(pool)
        .await
}
