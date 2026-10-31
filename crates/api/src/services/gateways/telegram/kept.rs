//! The bot's messages that show actions with their buttons, kept (ADR-148):
//! a pause's, in the conversation held in a person's private chat, or a
//! notice's, to the manager the requests await; with the requests each
//! shows and how many others it said of, so that it is written again once
//! some of them are settled (`rewrite`). Their buttons decide on what
//! Telegram holds of them (ADR-136): a message not kept decides all the
//! same, only its buttons are then taken off once decided, its text left
//! as it was.

use mimorya_db::repositories::gateway_cards::{CardPlace, WrittenCard, record_gateway_card};
use uuid::Uuid;

use crate::services::gateways::GatewayService;

/// A message the bot wrote with buttons: where, and its number in the
/// private chat as Telegram gave it.
#[derive(Debug, Clone, Copy)]
pub struct WrittenMessage {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    /// The person's private chat: their Telegram identifier.
    pub chat: i64,
    pub number: i64,
    pub place: CardPlace,
}

/// Keeps that `written` shows the requests `ids`, with `others`
/// (`in_application`, `managed`) said of beside them; a failure to keep it
/// only leaves it as written once decided, its buttons taken off.
pub async fn keep_message(
    gateways: &GatewayService,
    written: WrittenMessage,
    ids: &[Uuid],
    others: (i32, i32),
) {
    let (chat, number) = (written.chat.to_string(), written.number.to_string());
    let card = WrittenCard {
        tenant_id: written.tenant_id,
        gateway_id: written.gateway_id,
        chat: &chat,
        activity_id: &number,
        place: written.place,
    };
    let kept = async {
        let mut tx = gateways.db.begin(written.tenant_id).await?;
        record_gateway_card(&mut tx, card, ids, others).await?;
        tx.commit().await?;
        anyhow::Ok(())
    };
    if let Err(error) = kept.await {
        let (gateway, error) = (written.gateway_id, format!("{error:#}"));
        tracing::warn!(%gateway, %error, "Telegram message not kept: not written again once decided");
    }
}
