//! What one person wrote or set the assistant to do, as the export of their
//! own data reads it (ADR-187): their conversations and every message in
//! them, their personal memories, their scheduled tasks, triggers and
//! missions. A trigger's token is never read: only its hash is kept, and
//! it is not theirs to export. Read in the person's company's transaction,
//! for the person alone: a colleague's conversation shared with their unit
//! is the colleague's.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedConversation {
    pub id: Uuid,
    pub title: String,
    pub visibility: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedMessage {
    pub conversation_id: Uuid,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedMemory {
    pub kind: String,
    pub title: String,
    pub content: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedTask {
    pub title: String,
    pub instruction: String,
    pub weekdays: Vec<i16>,
    pub time_of_day: String,
    pub time_zone: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedTrigger {
    pub title: String,
    pub instruction: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedMission {
    pub title: String,
    pub description: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
}

/// The person's own conversations, the oldest first.
pub async fn exported_conversations(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedConversation>> {
    sqlx::query_as(
        "select id, title, visibility, created_at, updated_at, archived_at from conversations \
         where owner_user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

/// Every message of the person's own conversations, conversation by
/// conversation, in the order they were written.
pub async fn exported_messages(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedMessage>> {
    sqlx::query_as(
        "select m.conversation_id, m.role, m.content, m.created_at from messages m \
         join conversations c on c.tenant_id = m.tenant_id and c.id = m.conversation_id \
         where c.owner_user_id = $1 order by m.conversation_id, m.created_at, m.id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

/// The person's personal memories, the oldest first.
pub async fn exported_memories(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedMemory>> {
    sqlx::query_as(
        "select kind, title, content, status, created_at, expires_at from memory_items \
         where scope = 'user' and owner_user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_tasks(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedTask>> {
    sqlx::query_as(
        "select title, instruction, weekdays, time_of_day, time_zone, created_at \
         from scheduled_tasks where user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_triggers(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedTrigger>> {
    sqlx::query_as(
        "select title, instruction, enabled, created_at from inbound_hooks \
         where user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_missions(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedMission>> {
    sqlx::query_as(
        "select title, description, status, created_at, finished_at, result from missions \
         where owner_user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}
