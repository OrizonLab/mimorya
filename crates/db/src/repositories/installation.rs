//! What the operator's installation check reads of the database (ADR-223):
//! the migrations applied, the stack's processes listening, the archive of
//! the WAL. Nothing of any company.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::notifications::{RUN_EVENTS_LISTENER, RUN_QUEUE_LISTENER};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AppliedMigration {
    pub version: i64,
    pub success: bool,
    pub checksum: Vec<u8>,
}

/// The migration journal, through the one function that reads it for the
/// application role.
pub async fn applied_migrations(pool: &PgPool) -> sqlx::Result<Vec<AppliedMigration>> {
    sqlx::query_as(
        "select version, success, checksum from system_applied_migrations() order by version",
    )
    .fetch_all(pool)
    .await
}

/// Whether the role the API connects with escapes the companies' isolation:
/// a superuser, a role that bypasses row-level security, or the owner of a
/// table whose security is not forced on its owner (or one of its members,
/// who may act as it). The owner's address given in place of the
/// application's is one, superuser or not.
pub async fn role_escapes_isolation(pool: &PgPool) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select r.rolsuper or r.rolbypassrls or exists ( \
             select 1 from pg_class c join pg_namespace n on n.oid = c.relnamespace \
              where n.nspname = 'public' and c.relrowsecurity \
                and not c.relforcerowsecurity \
                and pg_has_role(current_user, c.relowner, 'MEMBER')) \
           from pg_roles r where r.rolname = current_user",
    )
    .fetch_one(pool)
    .await
}

/// The connections that listen as the stack's processes do: one per worker
/// on the run queue, one per API process on the run events (ADR-085).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listeners {
    pub queue: i64,
    pub events: i64,
}

pub async fn listeners(pool: &PgPool) -> sqlx::Result<Listeners> {
    let (queue, events): (i64, i64) = sqlx::query_as(
        "select count(*) filter (where application_name = $1), \
                count(*) filter (where application_name = $2) \
           from pg_stat_activity where datname = current_database()",
    )
    .bind(RUN_QUEUE_LISTENER)
    .bind(RUN_EVENTS_LISTENER)
    .fetch_one(pool)
    .await?;
    Ok(Listeners { queue, events })
}

/// Whether the WAL is archived (ADR-206), and how it went.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct WalArchive {
    pub mode: String,
    pub archived: i64,
    pub failed: i64,
    pub last_archived_at: Option<DateTime<Utc>>,
    pub last_failed_at: Option<DateTime<Utc>>,
}

pub async fn wal_archive(pool: &PgPool) -> sqlx::Result<WalArchive> {
    sqlx::query_as(
        "select current_setting('archive_mode') as mode, \
                archived_count as archived, failed_count as failed, \
                last_archived_time as last_archived_at, last_failed_time as last_failed_at \
           from pg_stat_archiver",
    )
    .fetch_one(pool)
    .await
}
