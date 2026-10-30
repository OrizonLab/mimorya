//! The cards the company's Teams bot showed actions awaiting a decision on,
//! with their buttons: a pause's, in the conversation held in a thread
//! (ADR-141), or a notice's, to the manager the requests await (ADR-142).
//! A button pressed names its card, not what the card showed: only a
//! request shown on a card of the bot's own, in the personal conversation
//! it was pressed in, is decided there. Only identifiers and counts are
//! kept, so that the card can be written again once some of its requests
//! are decided.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A card the bot wrote: the requests it shows, in order, how many others
/// its message said are to validate in the application (`in_application`)
/// or await a manager (`managed`), and what it was written with.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GatewayCard {
    pub approval_ids: Vec<Uuid>,
    pub in_application: i32,
    pub managed: i32,
    /// A pause's: the conversation it was written for.
    pub conversation_id: Option<Uuid>,
    /// A notice's: whom it was written to.
    pub notice_for: Option<Uuid>,
}

/// Where a card was written: with the pause of the conversation held in a
/// thread, or with a notice to a manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardPlace {
    Thread(Uuid),
    Notice(Uuid),
}

/// The card `activity_id` the Teams bot `gateway_id` wrote in its personal
/// conversation `chat`.
#[derive(Debug, Clone, Copy)]
pub struct WrittenCard<'a> {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    pub chat: &'a str,
    pub activity_id: &'a str,
    pub place: CardPlace,
}

/// Keeps that the card `written` shows the requests `approval_ids`, with
/// `others` (`in_application`, `managed`) said of beside it.
pub async fn record_gateway_card(
    conn: &mut PgConnection,
    written: WrittenCard<'_>,
    approval_ids: &[Uuid],
    (in_application, managed): (i32, i32),
) -> sqlx::Result<()> {
    let (thread_id, user_id) = match written.place {
        CardPlace::Thread(thread_id) => (Some(thread_id), None),
        CardPlace::Notice(user_id) => (None, Some(user_id)),
    };
    sqlx::query(
        "insert into gateway_cards (tenant_id, gateway_id, channel_id, activity_id, \
         thread_id, user_id, approval_ids, in_application, managed) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         on conflict (gateway_id, channel_id, activity_id) do nothing",
    )
    .bind(written.tenant_id)
    .bind(written.gateway_id)
    .bind(written.chat)
    .bind(written.activity_id)
    .bind(thread_id)
    .bind(user_id)
    .bind(approval_ids)
    .bind(in_application)
    .bind(managed)
    .execute(conn)
    .await?;
    Ok(())
}

/// The card `activity_id`, if it is one the Teams bot `gateway_id` wrote in
/// its personal conversation `chat`.
pub async fn find_gateway_card(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    chat: &str,
    activity_id: &str,
) -> sqlx::Result<Option<GatewayCard>> {
    sqlx::query_as(
        "select c.approval_ids, c.in_application, c.managed, t.conversation_id, \
         c.user_id as notice_for \
         from gateway_cards c \
         left join gateway_threads t on t.id = c.thread_id \
         where c.gateway_id = $1 and c.channel_id = $2 and c.activity_id = $3",
    )
    .bind(gateway_id)
    .bind(chat)
    .bind(activity_id)
    .fetch_optional(conn)
    .await
}

/// Forgets, in every company, the cards of notices written before `before`,
/// and those no longer in the conversation their manager is written in;
/// how many.
pub async fn purge_notice_cards(pool: &PgPool, before: DateTime<Utc>) -> sqlx::Result<i64> {
    sqlx::query_scalar("select system_purge_notice_cards($1)")
        .bind(before)
        .fetch_one(pool)
        .await
}
