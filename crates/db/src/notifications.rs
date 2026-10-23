//! PostgreSQL notifications (`LISTEN`/`NOTIFY`) that carry run events from
//! the process recording them to every other one (ADR-083): a notice sent
//! in a transaction reaches the listeners once, and only if, it commits.
//! The listener holds its own connection, out of the application's pool,
//! and connects again by itself when it loses it.

use std::str::FromStr;

use sqlx::PgExecutor;
use sqlx::postgres::{PgConnectOptions, PgListener, PgPoolOptions};

/// The channel run events are announced on.
pub const RUN_EVENTS_CHANNEL: &str = "mimorya_run_events";

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

/// Listens on the run events channel, on a connection of its own named
/// `application_name`.
pub async fn listen_run_events(url: &str, application_name: &str) -> sqlx::Result<PgListener> {
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
    listener.listen(RUN_EVENTS_CHANNEL).await?;
    Ok(listener)
}
