//! A conversation's summary and the messages it does not cover yet
//! (ADR-145): the history replayed to the model is the summary, then the
//! latest of those messages; the summary is folded further from the
//! earliest. A summary moves forward only from the one it was folded from:
//! two passes at once keep the first stored.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::conversations::{MESSAGE_COLUMNS, MessageRow};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SummaryRow {
    pub content: String,
    /// The last message it covers.
    pub through_message_id: Uuid,
    pub folded_messages: i32,
    pub updated_at: DateTime<Utc>,
}

pub async fn conversation_summary(
    conn: &mut PgConnection,
    conversation_id: Uuid,
) -> sqlx::Result<Option<SummaryRow>> {
    sqlx::query_as(
        "select content, through_message_id, folded_messages, updated_at \
         from conversation_summaries where conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(conn)
    .await
}

/// Messages after `after` (all of them when `None`), oldest first: the
/// `limit` latest, or the `limit` earliest.
async fn messages_after(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    after: Option<Uuid>,
    limit: i64,
    latest: bool,
) -> sqlx::Result<Vec<MessageRow>> {
    let order = if latest { "desc" } else { "asc" };
    let mut rows: Vec<MessageRow> = sqlx::query_as(&format!(
        "select {MESSAGE_COLUMNS} from messages where conversation_id = $1 \
         and ($2::uuid is null or (created_at, id) > \
           (select created_at, id from messages where id = $2)) \
         order by created_at {order}, id {order} limit $3"
    ))
    .bind(conversation_id)
    .bind(after)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    if latest {
        rows.reverse();
    }
    Ok(rows)
}

/// The `limit` latest messages after `after`, oldest first: the history's.
pub async fn latest_messages_after(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> sqlx::Result<Vec<MessageRow>> {
    messages_after(conn, conversation_id, after, limit, true).await
}

/// The `limit` earliest messages after `after`, oldest first: those to fold.
pub async fn earliest_messages_after(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> sqlx::Result<Vec<MessageRow>> {
    messages_after(conn, conversation_id, after, limit, false).await
}

/// How many messages come after `after` (all of them when `None`): those a
/// summary stopping there does not cover yet.
pub async fn count_messages_after(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    after: Option<Uuid>,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select count(*) from messages where conversation_id = $1 \
         and ($2::uuid is null or (created_at, id) > \
           (select created_at, id from messages where id = $2))",
    )
    .bind(conversation_id)
    .bind(after)
    .fetch_one(conn)
    .await
}

#[derive(Debug, Clone)]
pub struct FoldedSummary {
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub content: String,
    pub through_message_id: Uuid,
    /// Messages folded in by this pass.
    pub folded: i32,
}

/// Stores `summary` if the conversation's summary still stops at `from`
/// (has none when `None`): whether it was stored.
pub async fn store_summary(
    conn: &mut PgConnection,
    summary: FoldedSummary,
    from: Option<Uuid>,
) -> sqlx::Result<bool> {
    let stored = match from {
        None => {
            sqlx::query(
                "insert into conversation_summaries (conversation_id, tenant_id, content, \
                 through_message_id, folded_messages) values ($1, $2, $3, $4, $5) \
                 on conflict (conversation_id) do nothing",
            )
            .bind(summary.conversation_id)
            .bind(summary.tenant_id)
            .bind(&summary.content)
            .bind(summary.through_message_id)
            .bind(summary.folded)
            .execute(conn)
            .await?
        }
        Some(from) => {
            sqlx::query(
                "update conversation_summaries set content = $2, through_message_id = $3, \
                 folded_messages = folded_messages + $4, updated_at = now() \
                 where conversation_id = $1 and through_message_id = $5",
            )
            .bind(summary.conversation_id)
            .bind(&summary.content)
            .bind(summary.through_message_id)
            .bind(summary.folded)
            .bind(from)
            .execute(conn)
            .await?
        }
    };
    Ok(stored.rows_affected() == 1)
}
