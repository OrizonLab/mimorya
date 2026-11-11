//! Retention applied across the companies (DATA_MODEL §5, ADR-185), by
//! `system_*` functions whose horizons the database fixes: the sessions
//! ended thirty days ago; and past each company's own durations (ADR-195,
//! the platform's without its choice: a year, thirty days, twenty-four
//! months) the episodes and the memories past their own term, the personal
//! memories of an account disabled, the conversations idle, whole
//! (ADR-186); and the consumption and the oldest places of each audit chain
//! past the platform's five years, which no company sets (ADR-197,
//! ADR-198). What each company lost is counted, never read.

use sqlx::PgPool;
use uuid::Uuid;

/// The memories a company lost to a pass, by reason.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PurgedMemories {
    pub tenant_id: Uuid,
    /// Episodes past their company's duration, or memories past their own
    /// term.
    pub aged: i32,
    /// Personal memories of an account disabled past its company's
    /// duration.
    pub departed: i32,
}

/// The conversations a company lost to a pass, idle past its duration, and
/// the messages they held.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PurgedConversations {
    pub tenant_id: Uuid,
    pub conversations: i32,
    pub messages: i32,
}

/// The consumption records a company lost to a statement, older than five
/// years.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PurgedUsage {
    pub tenant_id: Uuid,
    pub records: i32,
}

/// The oldest places of its audit chain a company lost to a statement,
/// five years old, and the last of them.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PurgedAudit {
    pub tenant_id: Uuid,
    pub entries: i32,
    pub through_seq: i64,
}

/// Deletes the sessions ended thirty days ago; how many.
pub async fn purge_ended_sessions(pool: &PgPool) -> sqlx::Result<i32> {
    sqlx::query_scalar("select system_purge_ended_sessions()")
        .fetch_one(pool)
        .await
}

/// Deletes the memories past their retention; what each company lost.
pub async fn purge_retained_memories(pool: &PgPool) -> sqlx::Result<Vec<PurgedMemories>> {
    sqlx::query_as("select tenant_id, aged, departed from system_purge_retained_memories()")
        .fetch_all(pool)
        .await
}

/// Deletes the conversations idle past their company's duration, with all
/// they hold, the oldest first, `batch` at most (the database keeps it from 1 to
/// 500); what each company lost.
pub async fn purge_idle_conversations(
    pool: &PgPool,
    batch: i32,
) -> sqlx::Result<Vec<PurgedConversations>> {
    sqlx::query_as(
        "select tenant_id, conversations, messages from system_purge_idle_conversations($1)",
    )
    .bind(batch)
    .fetch_all(pool)
    .await
}

/// Deletes the consumption records older than five years, the oldest of
/// all the companies first, `batch` at most (the database keeps it from 1
/// to 5000); what each company lost.
pub async fn purge_old_usage(pool: &PgPool, batch: i32) -> sqlx::Result<Vec<PurgedUsage>> {
    sqlx::query_as("select tenant_id, records from system_purge_old_usage($1)")
        .bind(batch)
        .fetch_all(pool)
        .await
}

/// Deletes the oldest places of each company's audit chain five years old,
/// a prefix of it, never its last sealed place, `batch` at most in all (the
/// database keeps it from 1 to 5000); what each company lost.
pub async fn purge_old_audit_entries(pool: &PgPool, batch: i32) -> sqlx::Result<Vec<PurgedAudit>> {
    sqlx::query_as("select tenant_id, entries, through_seq from system_purge_old_audit_entries($1)")
        .bind(batch)
        .fetch_all(pool)
        .await
}
