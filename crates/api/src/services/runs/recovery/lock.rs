//! The run executor's lock (ADR-053): a PostgreSQL session advisory lock
//! that the process taking runs up holds on a connection of its own for its
//! whole life. A single instance executes runs (ADR-016): a second process
//! does not get it, and refuses to start rather than take up, or race, the
//! runs the first one executes. The connection is named, to be found in
//! `pg_stat_activity`, and probed by the server: should the API's host
//! vanish without closing it, the lock is freed within minutes, not hours.

use std::time::Duration;

use sqlx::PgConnection;

use crate::services::runs::RunService;

const RUN_EXECUTOR_LOCK: i64 = 0x6d69_6d6f_7279_6101;

/// The lock's connection: named, and probed every 30 s after a minute idle.
const LOCK_SESSION: &str = "set application_name = 'mimorya-run-executor'; \
     set tcp_keepalives_idle = 60; set tcp_keepalives_interval = 30; \
     set tcp_keepalives_count = 4";

/// A process that just stopped may hold the lock for a moment more.
const LOCK_ATTEMPTS: u32 = 6;
const LOCK_RETRY: Duration = Duration::from_millis(500);

/// Why an `api` process takes no run up (ADR-085).
pub const NOT_AN_EXECUTOR: &str =
    "An api process (MIMORYA_ROLE=api) executes no runs: a worker executes those it queues";

/// Why a second process does not start.
pub const LOCK_HELD_ELSEWHERE: &str = "Another API process holds the run executor's lock on this database (connection 'mimorya-run-executor' in pg_stat_activity): a single instance executes runs (ADR-016)";

impl RunService {
    /// Whether this process holds the run executor's lock: it alone expires
    /// requests and reminds managers (ADR-061, ADR-062).
    pub async fn executes(&self) -> bool {
        self.executor_lock.lock().await.is_some()
    }

    /// Takes the run executor's lock for the life of this process; fails
    /// while another process holds it.
    pub async fn claim_execution(&self) -> anyhow::Result<()> {
        if self.queues {
            anyhow::bail!(NOT_AN_EXECUTOR);
        }
        let mut held = self.executor_lock.lock().await;
        if held.is_some() {
            return Ok(());
        }
        let mut connection: PgConnection = self.db.pool().acquire().await?.detach();
        sqlx::raw_sql(LOCK_SESSION).execute(&mut connection).await?;
        for attempt in 1..=LOCK_ATTEMPTS {
            let taken: bool = sqlx::query_scalar("select pg_try_advisory_lock($1)")
                .bind(RUN_EXECUTOR_LOCK)
                .fetch_one(&mut connection)
                .await?;
            if taken {
                *held = Some(connection);
                return Ok(());
            }
            if attempt < LOCK_ATTEMPTS {
                tokio::time::sleep(LOCK_RETRY).await;
            }
        }
        anyhow::bail!(LOCK_HELD_ELSEWHERE)
    }
}
