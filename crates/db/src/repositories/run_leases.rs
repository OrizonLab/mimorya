//! Execution leases (ADR-083): the process executing a run holds it, its
//! executor named and its lease due, renewed while it executes; given up as
//! soon as the run leaves `running` (a trigger sees to it). A run whose
//! lease has expired is taken up by an executor.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::runs::InterruptedRun;

/// Takes the lease of `run_id` for `owner`, for `ttl_seconds`, whoever held
/// it: in the transaction that sets the run running, or that takes it up
/// once locked, its former holder known to be gone.
pub async fn take_run_lease(
    conn: &mut PgConnection,
    run_id: Uuid,
    owner: Uuid,
    ttl_seconds: f64,
) -> sqlx::Result<()> {
    sqlx::query(
        "update agent_runs set lease_owner = $2, \
         lease_expires_at = now() + make_interval(secs => $3) \
         where id = $1 and status = 'running'",
    )
    .bind(run_id)
    .bind(owner)
    .bind(ttl_seconds)
    .execute(conn)
    .await?;
    Ok(())
}

/// Renews, for `ttl_seconds` more, the leases `owner` holds on `run_ids`;
/// how many it still held.
pub async fn renew_run_leases(
    conn: &mut PgConnection,
    owner: Uuid,
    run_ids: &[Uuid],
    ttl_seconds: f64,
) -> sqlx::Result<u64> {
    let renewed = sqlx::query(
        "update agent_runs set lease_expires_at = now() + make_interval(secs => $3) \
         where id = any($1) and lease_owner = $2 and status = 'running'",
    )
    .bind(run_ids)
    .bind(owner)
    .bind(ttl_seconds)
    .execute(conn)
    .await?;
    Ok(renewed.rows_affected())
}

/// Every company's runs whose lease has expired by `now`, the longest
/// expired first, through a narrowly scoped database function: each is
/// then taken up in its own company.
pub async fn expired_run_leases(
    pool: &PgPool,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<InterruptedRun>> {
    sqlx::query_as("select tenant_id, run_id from system_expired_run_leases($1)")
        .bind(now)
        .fetch_all(pool)
        .await
}
