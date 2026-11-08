//! Retention applied across the companies (DATA_MODEL §5, ADR-185), by
//! `system_*` functions whose horizons the database fixes: the sessions
//! ended thirty days ago; the episodes recorded a year ago and the memories
//! past their own term; the personal memories of an account disabled thirty
//! days ago; the conversations idle for twenty-four months, whole (ADR-186).
//! What each company lost is counted, never read.

use sqlx::PgPool;
use uuid::Uuid;

/// The memories a company lost to a pass, by reason.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PurgedMemories {
    pub tenant_id: Uuid,
    /// Episodes a year old, or memories past their own term.
    pub aged: i32,
    /// Personal memories of an account disabled thirty days ago.
    pub departed: i32,
}

/// The conversations a company lost to a pass, idle for twenty-four
/// months, and the messages they held.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PurgedConversations {
    pub tenant_id: Uuid,
    pub conversations: i32,
    pub messages: i32,
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

/// Deletes the conversations idle for twenty-four months, with all they
/// hold, the oldest first, `batch` at most (the database keeps it from 1 to
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
