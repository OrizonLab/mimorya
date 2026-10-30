//! The cards the company's Teams bot showed a pause's actions on, with
//! their buttons (ADR-141): which requests each shows, in the conversation
//! held in a thread. A button pressed names its card, not what the card
//! showed: only a request shown on a card of the bot's own, in the
//! personal conversation it was pressed in, is decided there. Only
//! identifiers and counts are kept, so that the card can be written again
//! as it was once some of its requests are decided.

use sqlx::PgConnection;
use uuid::Uuid;

/// A card the bot wrote: the requests it shows, in order, and how many
/// others its pause said are to validate in the application
/// (`in_application`) or await a manager (`managed`).
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GatewayCard {
    pub approval_ids: Vec<Uuid>,
    pub in_application: i32,
    pub managed: i32,
    /// The conversation it was written for.
    pub conversation_id: Uuid,
}

/// Keeps that the card `activity_id` the bot wrote in the conversation held
/// in `thread_id` shows `card`.
pub async fn record_gateway_card(
    conn: &mut PgConnection,
    (tenant_id, thread_id): (Uuid, Uuid),
    activity_id: &str,
    card: &GatewayCard,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into gateway_cards \
         (tenant_id, thread_id, activity_id, approval_ids, in_application, managed) \
         values ($1, $2, $3, $4, $5, $6) on conflict (thread_id, activity_id) do nothing",
    )
    .bind(tenant_id)
    .bind(thread_id)
    .bind(activity_id)
    .bind(&card.approval_ids)
    .bind(card.in_application)
    .bind(card.managed)
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
        "select c.approval_ids, c.in_application, c.managed, t.conversation_id \
         from gateway_cards c \
         join gateway_threads t on t.id = c.thread_id \
         where t.gateway_id = $1 and t.channel_id = $2 and t.kind = 'teams' \
         and c.activity_id = $3 \
         order by c.created_at desc limit 1",
    )
    .bind(gateway_id)
    .bind(chat)
    .bind(activity_id)
    .fetch_optional(conn)
    .await
}
