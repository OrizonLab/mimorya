//! The threads a person's conversations with the agent are held in, on a
//! gateway's service (ADR-127): in their direct conversation with the
//! company's Slack application, a thread opened by their message outside
//! any thread, named after it, followed by their replies in it. One
//! conversation per thread, one thread per conversation. A thread is active
//! at its latest turn or answer: the answers pass reads the threads active
//! lately.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GatewayThreadRow {
    pub id: Uuid,
    pub gateway_id: Uuid,
    pub user_id: Uuid,
    pub conversation_id: Uuid,
    /// The Slack user who wrote, linked to the person then.
    pub external_id: String,
    /// Their direct conversation with the application (`D…`).
    pub channel_id: String,
    /// The thread: its first message's timestamp.
    pub thread_ts: String,
    pub created_at: DateTime<Utc>,
}

const SELECT: &str = "select id, gateway_id, user_id, conversation_id, external_id, channel_id, \
     thread_ts, created_at from gateway_threads";

pub struct NewGatewayThread<'a> {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    pub user_id: Uuid,
    pub conversation_id: Uuid,
    pub external_id: &'a str,
    pub channel_id: &'a str,
    pub thread_ts: &'a str,
}

/// The thread `thread_ts` of the direct conversation `channel_id`, on the
/// gateway, if a conversation is held in it.
pub async fn find_gateway_thread(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    channel_id: &str,
    thread_ts: &str,
) -> sqlx::Result<Option<GatewayThreadRow>> {
    sqlx::query_as(&format!(
        "{SELECT} where gateway_id = $1 and channel_id = $2 and thread_ts = $3"
    ))
    .bind(gateway_id)
    .bind(channel_id)
    .bind(thread_ts)
    .fetch_optional(conn)
    .await
}

/// Holds the conversation in the thread; `false` when the thread already
/// holds one (a message of the same thread came first).
pub async fn insert_gateway_thread(
    conn: &mut PgConnection,
    thread: NewGatewayThread<'_>,
) -> sqlx::Result<bool> {
    let inserted = sqlx::query(
        "insert into gateway_threads (tenant_id, gateway_id, user_id, conversation_id, \
         external_id, channel_id, thread_ts) values ($1, $2, $3, $4, $5, $6, $7) \
         on conflict (gateway_id, channel_id, thread_ts) do nothing",
    )
    .bind(thread.tenant_id)
    .bind(thread.gateway_id)
    .bind(thread.user_id)
    .bind(thread.conversation_id)
    .bind(thread.external_id)
    .bind(thread.channel_id)
    .bind(thread.thread_ts)
    .execute(conn)
    .await?;
    Ok(inserted.rows_affected() == 1)
}

/// The conversation's thread, if any, active now: a turn was sent there.
pub async fn touch_gateway_thread(
    conn: &mut PgConnection,
    conversation_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query("update gateway_threads set active_at = now() where conversation_id = $1")
        .bind(conversation_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// The service each of `conversation_ids` held in a thread is held on: its
/// gateway's kind (`slack`), by conversation.
pub async fn conversations_held(
    conn: &mut PgConnection,
    conversation_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, String>> {
    let held: Vec<(Uuid, String)> = sqlx::query_as(
        "select t.conversation_id, g.kind from gateway_threads t \
         join gateways g on g.id = t.gateway_id where t.conversation_id = any($1)",
    )
    .bind(conversation_ids)
    .fetch_all(conn)
    .await?;
    Ok(held.into_iter().collect())
}
