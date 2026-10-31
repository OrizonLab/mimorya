//! The requests awaiting a manager told in Telegram (ADR-137): a message in
//! their private chat with the company's bot, in the HTML Telegram reads
//! (`telegram::approver_notice`), or in plain text once Telegram refused
//! it; its actions that may be validated there, as many as one message
//! holds, with their exact parameters and their buttons under it (ADR-136),
//! kept to be written again once some of them are settled (ADR-148).
//! Telegram proves no address: the Telegram account the person accepted to
//! link, knowing to whose account (ADR-133), is theirs until they unlink
//! it, or it is linked to someone else.

use mimorya_core::gateways::telegram::{approval_keyboard, shown_in_message};
use mimorya_db::repositories::gateway_approval_notices::GatewayApprovalNotice;
use mimorya_db::repositories::gateway_cards::CardPlace;
use mimorya_db::repositories::gateway_replies::ReplyRecord;
use uuid::Uuid;

use super::{Reach, Told, shown_action};
use crate::app::Services;
use crate::services::gateways::telegram::{
    BotChat, WrittenMessage, after_failure, approver_notice, keep_message, write_in_chat,
};

/// Writes `told.to`, in their private chat with the bot (the Telegram
/// account `reach.person`), the requests `theirs`; what became of the
/// message.
pub(super) async fn tell(
    services: &Services,
    told: &Told<'_>,
    reach: Reach<'_>,
    theirs: &[GatewayApprovalNotice],
) -> ReplyRecord {
    // Their identifier, as linked: their private chat's.
    let Ok(chat) = reach.person.parse::<i64>() else {
        tracing::warn!(gateway = %told.gateway.id, "Telegram chat unreadable");
        return ReplyRecord::Done("failed");
    };
    let shown = shown_in_message(theirs.iter().map(shown_action));
    let link = services.gateways.approvals_url();
    let notice = told.notice(theirs, &shown, &link);
    let keyboard = (!shown.is_empty()).then(|| approval_keyboard(&shown));
    let to = BotChat {
        gateway_id: told.gateway.id,
        token: reach.token,
        chat,
    };
    let text = |markup| approver_notice(markup, &notice);
    match write_in_chat(&services.gateways, to, text, keyboard.as_ref()).await {
        Ok(number) => {
            if keyboard.is_some() {
                let written = WrittenMessage {
                    tenant_id: told.tenant_id,
                    gateway_id: told.gateway.id,
                    chat,
                    number,
                    place: CardPlace::Notice(told.to.user_id),
                };
                let ids: Vec<Uuid> = shown.iter().map(|a| a.approval_id).collect();
                let in_application = i32::try_from(theirs.len() - shown.len()).unwrap_or(i32::MAX);
                keep_message(&services.gateways, written, &ids, (in_application, 0)).await;
            }
            ReplyRecord::Done("sent")
        }
        Err(failure) => after_failure(&failure, reach.attempt, reach.now),
    }
}
