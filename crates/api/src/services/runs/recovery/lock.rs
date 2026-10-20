//! The run executor's lock (ADR-053): a PostgreSQL session advisory lock
//! that the process taking runs up holds on a connection of its own for its
//! whole life. A single instance executes runs (ADR-016): a second process
//! does not get it, and refuses to start rather than take up, or race, the
//! runs the first one executes.

use std::time::Duration;

use sqlx::PgConnection;

use crate::services::runs::RunService;

const RUN_EXECUTOR_LOCK: i64 = 0x6d69_6d6f_7279_6101;

/// A process that just stopped may hold the lock for a moment more.
const LOCK_ATTEMPTS: u32 = 6;
const LOCK_RETRY: Duration = Duration::from_millis(500);

/// Why a second process does not start.
pub const LOCK_HELD_ELSEWHERE: &str = "Another API process holds the run executor's lock on this database: a single instance executes runs (ADR-016)";

impl RunService {
    /// Takes the run executor's lock for the life of this process; fails
    /// while another process holds it.
    pub async fn claim_execution(&self) -> anyhow::Result<()> {
        let mut held = self.executor_lock.lock().await;
        if held.is_some() {
            return Ok(());
        }
        let mut connection: PgConnection = self.db.pool().acquire().await?.detach();
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
