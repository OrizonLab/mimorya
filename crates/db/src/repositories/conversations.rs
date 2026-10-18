//! Conversations and their messages.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Title given to a new conversation until its first message names it.
pub const DEFAULT_CONVERSATION_TITLE: &str = "Nouvelle conversation";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConversationRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub owner_user_id: Uuid,
    pub title: String,
    pub visibility: String,
    pub org_unit_id: Option<Uuid>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const CONVERSATION_COLUMNS: &str = "id, tenant_id, owner_user_id, title, visibility, org_unit_id, archived_at, created_at, updated_at";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MessageRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub role: String,
    pub author_user_id: Option<Uuid>,
    pub content: String,
    pub run_id: Option<Uuid>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

const MESSAGE_COLUMNS: &str =
    "id, tenant_id, conversation_id, role, author_user_id, content, run_id, metadata, created_at";

pub async fn list_conversations(
    conn: &mut PgConnection,
    owner_user_id: Uuid,
) -> sqlx::Result<Vec<ConversationRow>> {
    sqlx::query_as(&format!(
        "select {CONVERSATION_COLUMNS} from conversations \
         where owner_user_id = $1 and archived_at is null order by updated_at desc limit 100"
    ))
    .bind(owner_user_id)
    .fetch_all(conn)
    .await
}

pub async fn create_conversation(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    owner_user_id: Uuid,
    title: Option<&str>,
) -> sqlx::Result<ConversationRow> {
    sqlx::query_as(&format!(
        "insert into conversations (tenant_id, owner_user_id, title) values ($1, $2, $3) \
         returning {CONVERSATION_COLUMNS}"
    ))
    .bind(tenant_id)
    .bind(owner_user_id)
    .bind(title.unwrap_or(DEFAULT_CONVERSATION_TITLE))
    .fetch_one(conn)
    .await
}

pub async fn get_conversation(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<ConversationRow>> {
    sqlx::query_as(&format!(
        "select {CONVERSATION_COLUMNS} from conversations where id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Same as `get_conversation`, locking the row until the transaction ends:
/// concurrent turns of one conversation are serialised.
pub async fn lock_conversation(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<ConversationRow>> {
    sqlx::query_as(&format!(
        "select {CONVERSATION_COLUMNS} from conversations where id = $1 for update"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

#[derive(Debug, Clone, Default)]
pub struct ConversationPatch {
    pub title: Option<String>,
    /// `Some(None)` restores an archived conversation.
    pub archived_at: Option<Option<DateTime<Utc>>>,
}

pub async fn update_conversation(
    conn: &mut PgConnection,
    id: Uuid,
    patch: ConversationPatch,
) -> sqlx::Result<()> {
    sqlx::query(
        "update conversations set title = coalesce($2, title), \
         archived_at = case when $3 then $4 else archived_at end, updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(patch.title)
    .bind(patch.archived_at.is_some())
    .bind(patch.archived_at.flatten())
    .execute(conn)
    .await?;
    Ok(())
}

/// The last `limit` messages, oldest first.
pub async fn list_messages(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<MessageRow>> {
    let mut rows: Vec<MessageRow> = sqlx::query_as(&format!(
        "select {MESSAGE_COLUMNS} from messages where conversation_id = $1 \
         order by created_at desc limit $2"
    ))
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    rows.reverse();
    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    /// `user` or `assistant`.
    pub role: &'static str,
    pub content: String,
    pub author_user_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub metadata: Value,
}

pub async fn add_message(conn: &mut PgConnection, input: NewMessage) -> sqlx::Result<MessageRow> {
    let row = sqlx::query_as(&format!(
        "insert into messages (tenant_id, conversation_id, role, content, author_user_id, run_id, metadata) \
         values ($1, $2, $3, $4, $5, $6, $7) returning {MESSAGE_COLUMNS}"
    ))
    .bind(input.tenant_id)
    .bind(input.conversation_id)
    .bind(input.role)
    .bind(&input.content)
    .bind(input.author_user_id)
    .bind(input.run_id)
    .bind(&input.metadata)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("update conversations set updated_at = now() where id = $1")
        .bind(input.conversation_id)
        .execute(&mut *conn)
        .await?;
    Ok(row)
}
