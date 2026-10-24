//! Execution leases (ADR-083) as the database holds them: read, set as
//! another executor would have, and read from a model's step, which holds
//! the runtime's thread meanwhile.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use uuid::Uuid;

use super::owner_url;

pub type Lease = (Option<Uuid>, Option<DateTime<Utc>>);

/// The lease of `run_id`: its executor and until when.
pub async fn lease_of(run_id: Uuid) -> Lease {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let lease =
        sqlx::query_as("select lease_owner, lease_expires_at from agent_runs where id = $1")
            .bind(run_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    owner.close().await;
    lease
}

/// Gives `run_id`'s lease to `executor`, due `in_` from now on the
/// database's clock (past when negative), as another executor would have
/// taken it.
pub async fn lease_to(run_id: Uuid, executor: Uuid, in_: Duration) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let seconds = in_.num_milliseconds() as f64 / 1000.0;
    sqlx::query(
        "update agent_runs set lease_owner = $2, \
         lease_expires_at = clock_timestamp() + make_interval(secs => $3) where id = $1",
    )
    .bind(run_id)
    .bind(executor)
    .bind(seconds)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// The lease of the run `user_id`'s agent is running, read from another
/// thread: from a model's step, which holds the runtime's thread.
pub fn running_lease_of(user_id: Uuid) -> Lease {
    blocking(move |owner| async move {
        sqlx::query_as(
            "select lease_owner, lease_expires_at from agent_runs \
             where on_behalf_of_user_id = $1 and status = 'running'",
        )
        .bind(user_id)
        .fetch_one(&owner)
        .await
        .unwrap()
    })
}

/// A model's step that records the lease of the run `user_id`'s agent is
/// running, as it goes on.
pub fn lease_seen(seen: &Arc<Mutex<Option<Lease>>>, user_id: Uuid) -> ScriptedStep {
    let seen = seen.clone();
    ScriptedStep::dynamic(move |_| {
        *seen.lock().unwrap() = Some(running_lease_of(user_id));
        ScriptedReply::text("Repris.")
    })
}

/// Expires the lease of the run `user_id`'s agent is running, from
/// another thread.
pub fn expire_running_lease_of(user_id: Uuid) {
    blocking(move |owner| async move {
        sqlx::query(
            "update agent_runs set lease_expires_at = clock_timestamp() - interval '1 second' \
             where on_behalf_of_user_id = $1 and status = 'running'",
        )
        .bind(user_id)
        .execute(&owner)
        .await
        .unwrap();
    })
}

/// Hands the lease of the user's running run to `executor`, as another
/// executor taking it up would, from a model step.
pub fn hand_running_lease_to(user_id: Uuid, executor: Uuid) {
    blocking(move |owner| async move {
        sqlx::query(
            "update agent_runs set lease_owner = $2, \
             lease_expires_at = clock_timestamp() + interval '1 minute' \
             where on_behalf_of_user_id = $1 and status = 'running'",
        )
        .bind(user_id)
        .bind(executor)
        .execute(&owner)
        .await
        .unwrap();
    })
}

/// Pauses the user's running run, as another executor would have once it
/// took the run up: its lease goes with its running.
pub fn pause_running_run_of(user_id: Uuid) {
    blocking(move |owner| async move {
        sqlx::query(
            "update agent_runs set status = 'awaiting_approval' \
             where on_behalf_of_user_id = $1 and status = 'running'",
        )
        .bind(user_id)
        .execute(&owner)
        .await
        .unwrap();
    })
}

fn blocking<T, F, Fut>(query: F) -> T
where
    T: Send + 'static,
    F: FnOnce(sqlx::PgPool) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = T>,
{
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
                let value = query(owner.clone()).await;
                owner.close().await;
                value
            })
    })
    .join()
    .unwrap()
}
