//! PostgreSQL notifications (`LISTEN`/`NOTIFY`) that carry run events from
//! the process recording them to every other one (ADR-084): a notice sent
//! in a transaction reaches the listeners once, and only if, it commits.
//! The listener holds its own connection, out of the application's pool,
//! and connects again by itself when it loses it. PostgreSQL checks no
//! privilege on a channel: any role connected to the database may listen
//! or send on it, which only the application's does. A second channel wakes
//! the workers up when a run is queued (ADR-085).

use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgListener, PgPoolOptions};
use sqlx::{PgExecutor, PgPool};

/// The channel run events are announced on.
pub const RUN_EVENTS_CHANNEL: &str = "mimorya_run_events";
/// A process's connection listening on it, as an operator finds it.
pub const RUN_EVENTS_LISTENER: &str = "mimorya-run-events";
/// The channel that wakes the workers up when a run is queued (ADR-085).
pub const RUN_QUEUE_CHANNEL: &str = "mimorya_run_queue";
/// A worker's connection listening on it.
pub const RUN_QUEUE_LISTENER: &str = "mimorya-run-queue";

/// Announces `payload` on the run events channel: within a transaction,
/// once it commits.
pub async fn notify_run_events<'e>(
    executor: impl PgExecutor<'e>,
    payload: &str,
) -> sqlx::Result<()> {
    sqlx::query("select pg_notify($1, $2)")
        .bind(RUN_EVENTS_CHANNEL)
        .bind(payload)
        .execute(executor)
        .await?;
    Ok(())
}

/// Wakes the workers up: a run is queued, once the transaction commits.
pub async fn notify_run_queued<'e>(executor: impl PgExecutor<'e>) -> sqlx::Result<()> {
    sqlx::query("select pg_notify($1, '')")
        .bind(RUN_QUEUE_CHANNEL)
        .execute(executor)
        .await?;
    Ok(())
}

/// Listens on the run events channel, on a connection of its own named
/// `application_name`.
pub async fn listen_run_events(url: &str, application_name: &str) -> sqlx::Result<PgListener> {
    listen(url, application_name, RUN_EVENTS_CHANNEL).await
}

/// Listens on the run queue channel, on a connection of its own.
pub async fn listen_run_queue(url: &str) -> sqlx::Result<PgListener> {
    listen(url, RUN_QUEUE_LISTENER, RUN_QUEUE_CHANNEL).await
}

async fn listen(url: &str, application_name: &str, channel: &str) -> sqlx::Result<PgListener> {
    let connect = PgConnectOptions::from_str(url)?.application_name(application_name);
    // One connection, kept for as long as the listener lives.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .max_lifetime(None)
        .idle_timeout(None)
        .connect_with(connect)
        .await?;
    let mut listener = PgListener::connect_with(&pool).await?;
    // Its own pool: nobody else closes it.
    listener.ignore_pool_close_event(true);
    listener.listen(channel).await?;
    Ok(listener)
}

/// How many processes listen on the run events channel, in this database.
pub async fn run_event_listeners(pool: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select count(*) from pg_stat_activity \
         where application_name = $1 and datname = current_database()",
    )
    .bind(RUN_EVENTS_LISTENER)
    .fetch_one(pool)
    .await
}
