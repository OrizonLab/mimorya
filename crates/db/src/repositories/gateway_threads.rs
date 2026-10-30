//! The threads a person's conversations with the agent are held in, on a
//! gateway's service (ADR-127): in their direct conversation with the
//! company's Slack application, a thread opened by their message outside
//! any thread, named after it, followed by their replies in it. In their
//! private chat with the company's Telegram bot (ADR-135), one conversation
//! at a time: opened by their message, followed by the next ones until they
//! ask for a new one, which closes it; in their personal conversation with
//! the company's Teams bot alike (ADR-140), answered at the Teams service
//! its latest message came from. One conversation per thread, one
//! thread per conversation. A thread is active at its latest turn or
//! answer: the answers pass reads the threads active lately.

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
    /// The gateway's service: `slack`, `telegram` or `teams`.
    pub kind: String,
    /// Who wrote on that service, linked to the person then.
    pub external_id: String,
    /// Their direct conversation with the application (`D…`), their
    /// private chat with the bot (their Telegram identifier), or their
    /// personal conversation with the Teams bot (`a:…`).
    pub channel_id: String,
    /// The thread: its first message's timestamp in Slack, the number of
    /// the message that opened it in Telegram, that activity's identifier
    /// in Teams.
    pub thread_ts: String,
    /// The Teams service its latest message came from (Teams only).
    pub service_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

const SELECT: &str = "select id, gateway_id, user_id, conversation_id, kind, external_id, \
     channel_id, thread_ts, service_url, created_at from gateway_threads";

pub struct NewGatewayThread<'a> {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    pub user_id: Uuid,
    pub conversation_id: Uuid,
    pub external_id: &'a str,
    pub channel_id: &'a str,
    pub thread_ts: &'a str,
    /// The Teams service its message came from (Teams only).
    pub service_url: Option<&'a str>,
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

/// Holds the conversation in the thread, of its gateway's service; `false`
/// when the thread already holds one (a message of the same thread came
/// first), or the person's private chat with a Telegram bot another open
/// one (a message of theirs opened it first).
pub async fn insert_gateway_thread(
    conn: &mut PgConnection,
    thread: NewGatewayThread<'_>,
) -> sqlx::Result<bool> {
    let inserted = sqlx::query(
        "insert into gateway_threads (tenant_id, gateway_id, kind, user_id, conversation_id, \
         external_id, channel_id, thread_ts, service_url) \
         values ($1, $2, (select kind from gateways where id = $2), $3, $4, $5, $6, $7, $8) \
         on conflict do nothing",
    )
    .bind(thread.tenant_id)
    .bind(thread.gateway_id)
    .bind(thread.user_id)
    .bind(thread.conversation_id)
    .bind(thread.external_id)
    .bind(thread.channel_id)
    .bind(thread.thread_ts)
    .bind(thread.service_url)
    .execute(conn)
    .await?;
    Ok(inserted.rows_affected() == 1)
}

/// The person `user_id`'s open conversation in their private chat `chat`
/// with the Telegram bot `gateway_id` (ADR-135), or in their personal
/// conversation with the Teams bot (ADR-140), if any: one archived in the
/// application meanwhile is closed first, and none.
pub async fn open_gateway_chat(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    chat: &str,
    user_id: Uuid,
) -> sqlx::Result<Option<GatewayThreadRow>> {
    sqlx::query(
        "update gateway_threads t set closed_at = now() from conversations c \
         where c.id = t.conversation_id and c.archived_at is not null \
         and t.gateway_id = $1 and t.channel_id = $2 and t.user_id = $3 \
         and t.kind in ('telegram', 'teams') and t.closed_at is null",
    )
    .bind(gateway_id)
    .bind(chat)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query_as(&format!(
        "{SELECT} where gateway_id = $1 and channel_id = $2 and user_id = $3 \
         and kind in ('telegram', 'teams') and closed_at is null"
    ))
    .bind(gateway_id)
    .bind(chat)
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// Closes the person `user_id`'s open conversation in their private chat
/// `chat` with the Telegram bot `gateway_id`, or their personal
/// conversation with the Teams bot: their next message opens a new one.
/// Whether one was open.
pub async fn close_gateway_chat(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    chat: &str,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let closed = sqlx::query(
        "update gateway_threads set closed_at = now() where gateway_id = $1 \
         and channel_id = $2 and user_id = $3 and kind in ('telegram', 'teams') \
         and closed_at is null",
    )
    .bind(gateway_id)
    .bind(chat)
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(closed.rows_affected() > 0)
}

/// The Teams service the thread `thread_id`'s latest message came from
/// (ADR-140): its answers are written there.
pub async fn move_teams_thread(
    conn: &mut PgConnection,
    thread_id: Uuid,
    service_url: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "update gateway_threads set service_url = $2 where id = $1 and kind = 'teams' \
         and service_url is distinct from $2",
    )
    .bind(thread_id)
    .bind(service_url)
    .execute(conn)
    .await?;
    Ok(())
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
/// gateway's kind (`slack`, `telegram`, `teams`), by conversation. A
/// Telegram or Teams conversation closed since, or archived (closed by its
/// person's next message, ADR-135, ADR-140), is held there no more: their
/// messages to the bot go to another.
pub async fn conversations_held(
    conn: &mut PgConnection,
    conversation_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, String>> {
    let held: Vec<(Uuid, String)> = sqlx::query_as(
        "select t.conversation_id, g.kind from gateway_threads t \
         join gateways g on g.id = t.gateway_id \
         join conversations c on c.id = t.conversation_id \
         where t.conversation_id = any($1) and t.closed_at is null \
         and (t.kind = 'slack' or c.archived_at is null)",
    )
    .bind(conversation_ids)
    .fetch_all(conn)
    .await?;
    Ok(held.into_iter().collect())
}
