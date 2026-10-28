//! The answers in Slack to the runs a person started there (ADR-127): each
//! end or pause of such a run (its `run_finished` event) is answered once,
//! in the thread its conversation is held in, by the periodic pass, within
//! a while of it. What became of each is kept, never what was written:
//! that is in the conversation.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// An end or pause of a run, in some company, to answer in Slack.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct UnrepliedEvent {
    pub tenant_id: Uuid,
    pub event_id: i64,
}

/// Every company's ends and pauses since `since`, still to answer at
/// `now`, oldest first, at most `limit`, through a narrowly scoped
/// database function: each is then answered in its own company.
pub async fn unreplied_gateway_events(
    pool: &PgPool,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
    limit: i32,
) -> sqlx::Result<Vec<UnrepliedEvent>> {
    sqlx::query_as("select tenant_id, event_id from system_unreplied_gateway_events($1, $2, $3)")
        .bind(since)
        .bind(now)
        .bind(limit)
        .fetch_all(pool)
        .await
}

/// What answering an end or pause needs.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReplyTarget {
    pub event_id: i64,
    pub run_id: Uuid,
    /// How the run ended, or `awaiting_approval`.
    pub status: String,
    pub conversation_id: Uuid,
    pub thread_id: Uuid,
    pub gateway_id: Uuid,
    pub channel_id: String,
    pub thread_ts: String,
    pub gateway_enabled: bool,
    /// Whether the Slack user who wrote is still the person's identity.
    pub still_linked: bool,
    /// The run's latest message in the conversation, written by then.
    pub answer: Option<String>,
    /// The attempts made so far.
    pub attempts: i32,
}

/// The end or pause `event_id`, if it is one of a run started from a
/// gateway, in a conversation held in a thread.
pub async fn gateway_reply_target(
    conn: &mut PgConnection,
    event_id: i64,
) -> sqlx::Result<Option<ReplyTarget>> {
    sqlx::query_as(
        "select e.id as event_id, e.run_id, coalesce(e.data->>'status', '') as status, \
         t.conversation_id, t.id as thread_id, t.gateway_id, t.channel_id, t.thread_ts, \
         g.enabled as gateway_enabled, \
         exists (select 1 from gateway_identities i where i.gateway_id = t.gateway_id \
           and i.user_id = t.user_id and i.external_id = t.external_id) as still_linked, \
         (select m.content from messages m where m.conversation_id = t.conversation_id \
           and m.run_id = e.run_id and m.role = 'assistant' and m.created_at <= e.created_at \
           order by m.created_at desc limit 1) as answer, \
         coalesce(rep.attempts, 0) as attempts \
         from run_events e \
         join agent_runs r on r.id = e.run_id \
         join gateway_threads t on t.conversation_id = r.conversation_id \
         join gateways g on g.id = t.gateway_id \
         left join gateway_replies rep on rep.event_id = e.id \
         where e.id = $1 and e.type = 'run_finished' and r.channel = 'slack'",
    )
    .bind(event_id)
    .fetch_optional(conn)
    .await
}

/// What became of an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyRecord {
    /// `sent`, `refused`, `failed` or `skipped`: never tried again.
    Done(&'static str),
    /// To try again from then.
    Retry(DateTime<Utc>),
}

/// Records what became of the answer to `event_id`, one attempt more.
pub async fn record_gateway_reply(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    event_id: i64,
    thread_id: Uuid,
    record: ReplyRecord,
) -> sqlx::Result<()> {
    let (outcome, retry_after) = match record {
        ReplyRecord::Done(outcome) => (Some(outcome), None),
        ReplyRecord::Retry(at) => (None, Some(at)),
    };
    sqlx::query(
        "insert into gateway_replies (event_id, tenant_id, thread_id, outcome, retry_after) \
         values ($1, $2, $3, $4, $5) on conflict (event_id) do update \
         set outcome = excluded.outcome, retry_after = excluded.retry_after, \
         attempts = gateway_replies.attempts + 1, updated_at = now()",
    )
    .bind(event_id)
    .bind(tenant_id)
    .bind(thread_id)
    .bind(outcome)
    .bind(retry_after)
    .execute(conn)
    .await?;
    Ok(())
}
