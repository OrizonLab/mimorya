//! A periodic sweep that runs in one process at a time, whatever the number
//! of workers (ADR-086): it takes a PostgreSQL session advisory lock named
//! after it, on a connection of its own, out of the pool. The lock goes
//! with the connection: released at the end of the sweep, and should the
//! process vanish, as soon as the server sees the connection gone: it
//! probes it, so that a vanished host frees the lock within minutes, not
//! hours. The connection is named after the sweep, to be found in
//! `pg_stat_activity`.

use std::time::Duration;

use sqlx::{Connection, PgConnection};

use crate::Database;

/// The lock of a sweep, held until released or dropped.
pub struct SweepLock {
    connection: PgConnection,
    key: String,
}

const KEY: &str = "hashtextextended('mimorya.sweep:' || $1, 0)";

/// The connection's name, and its probes: every 30 s after a minute idle,
/// given up after four unanswered.
const SESSION: &str = "select set_config('application_name', $1, false), \
     set_config('tcp_keepalives_idle', '60', false), \
     set_config('tcp_keepalives_interval', '30', false), \
     set_config('tcp_keepalives_count', '4', false)";

impl Database {
    /// The lock of the sweep `name`, if no other process holds it.
    pub async fn try_sweep_lock(&self, name: &str) -> sqlx::Result<Option<SweepLock>> {
        let mut connection = self.sweep_connection(name).await?;
        let taken: bool = sqlx::query_scalar(&format!("select pg_try_advisory_lock({KEY})"))
            .bind(name)
            .fetch_one(&mut connection)
            .await?;
        if !taken {
            connection.close().await?;
            return Ok(None);
        }
        Ok(Some(SweepLock {
            connection,
            key: name.to_owned(),
        }))
    }

    /// The lock of the sweep `name`, once no other process holds it; fails
    /// should another hold it for longer than `wait`.
    pub async fn sweep_lock(&self, name: &str, wait: Duration) -> sqlx::Result<SweepLock> {
        let mut connection = self.sweep_connection(name).await?;
        sqlx::query("select set_config('lock_timeout', $1, false)")
            .bind(format!("{}ms", wait.as_millis().max(1)))
            .execute(&mut connection)
            .await?;
        sqlx::query(&format!("select pg_advisory_lock({KEY})"))
            .bind(name)
            .execute(&mut connection)
            .await?;
        Ok(SweepLock {
            connection,
            key: name.to_owned(),
        })
    }

    async fn sweep_connection(&self, name: &str) -> sqlx::Result<PgConnection> {
        let mut connection = self.pool().acquire().await?.detach();
        sqlx::query(SESSION)
            .bind(format!("mimorya-sweep-{name}"))
            .execute(&mut connection)
            .await?;
        Ok(connection)
    }
}

impl SweepLock {
    /// Whether the lock is still held: its connection answers, so the
    /// session that took it, which it goes with, is still alive.
    pub async fn held(&mut self) -> bool {
        sqlx::query("select 1")
            .execute(&mut self.connection)
            .await
            .is_ok()
    }

    /// Releases the lock, and closes its connection: should either fail,
    /// the connection is dropped, and the lock with it.
    pub async fn release(mut self) {
        let unlocked = sqlx::query(&format!("select pg_advisory_unlock({KEY})"))
            .bind(&self.key)
            .execute(&mut self.connection)
            .await;
        if unlocked.is_ok() {
            let _ = self.connection.close().await;
        }
    }
}
