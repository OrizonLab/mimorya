//! Execution leases (ADR-083): the process executing a run holds it, its
//! executor named and its lease due, renewed while it executes; given up as
//! soon as the run leaves `running` (a trigger sees to it). A run whose
//! lease has expired, or which never had one, is taken up by an executor
//! (ADR-086), and its taking up counts as progress against the sweep of
//! stuck runs. Leases are set and read on
//! the database's clock alone (`clock_timestamp()`): neither the executors'
//! clocks nor a wait for a lock shortens them.

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
         lease_expires_at = clock_timestamp() + make_interval(secs => $3) \
         where id = $1 and status = 'running'",
    )
    .bind(run_id)
    .bind(owner)
    .bind(ttl_seconds)
    .execute(conn)
    .await?;
    Ok(())
}

/// Records when `run_id` is taken up: the sweep of stuck runs counts it as
/// progress, and leaves it until its next step is due; how long it may stay
/// interrupted still counts from its last step (ADR-086).
pub async fn mark_taken_up(conn: &mut PgConnection, run_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("update agent_runs set taken_up_at = now() where id = $1")
        .bind(run_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Renews, for `ttl_seconds` more, the leases `owner` holds on `run_ids`;
/// the runs whose lease it still held.
pub async fn renew_run_leases(
    conn: &mut PgConnection,
    owner: Uuid,
    run_ids: &[Uuid],
    ttl_seconds: f64,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "update agent_runs set lease_expires_at = clock_timestamp() + make_interval(secs => $3) \
         where id = any($1) and lease_owner = $2 and status = 'running' returning id",
    )
    .bind(run_ids)
    .bind(owner)
    .bind(ttl_seconds)
    .fetch_all(conn)
    .await
}

/// Locks `run_id` until the transaction ends, if it still runs under
/// `owner`'s lease: what the transaction writes for it then commits under
/// that lease, which nobody can take meanwhile (ADR-086). Whether it does.
pub async fn lock_held_lease(
    conn: &mut PgConnection,
    run_id: Uuid,
    owner: Uuid,
) -> sqlx::Result<bool> {
    let held: Option<Uuid> = sqlx::query_scalar(
        "select id from agent_runs where id = $1 and lease_owner = $2 \
         and status = 'running' for no key update",
    )
    .bind(run_id)
    .bind(owner)
    .fetch_optional(conn)
    .await?;
    Ok(held.is_some())
}

/// Whether `run_id`'s lease has expired, or it never had one (ADR-086), as
/// it stands now (read under the run's lock, after any renewal it waited
/// for).
pub async fn run_lease_expired(conn: &mut PgConnection, run_id: Uuid) -> sqlx::Result<bool> {
    let expired: Option<bool> = sqlx::query_scalar(
        "select coalesce(lease_expires_at < clock_timestamp(), true) \
         from agent_runs where id = $1",
    )
    .bind(run_id)
    .fetch_optional(conn)
    .await?
    .flatten();
    Ok(expired == Some(true))
}

/// Every company's runs whose lease has expired or which never had one,
/// these first, then the longest expired, through a narrowly scoped
/// database function: each is then taken up in its own company.
pub async fn expired_run_leases(pool: &PgPool) -> sqlx::Result<Vec<InterruptedRun>> {
    sqlx::query_as("select tenant_id, run_id from system_expired_run_leases()")
        .fetch_all(pool)
        .await
}
