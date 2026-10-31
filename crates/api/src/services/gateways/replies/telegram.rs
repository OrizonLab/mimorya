//! An end or pause answered in the person's private chat with the
//! company's Telegram bot (ADR-135), where its conversation is held, in the
//! HTML Telegram reads (`telegram::reply_text`); in plain text when
//! Telegram does not read it. A pause shows the actions awaiting the
//! person's own validation that may be given there with their exact
//! parameters and their buttons (ADR-136), as many as one message holds
//! (`shown_in_message`); the others are validated in the application,
//! which shows them whole. Kept, it is written again once some of them
//! are settled (ADR-148).

use chrono::{DateTime, Utc};
use mimorya_core::gateways::telegram::{approval_keyboard, shown_in_message};
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_cards::CardPlace;
use mimorya_db::repositories::gateway_replies::{ReplyRecord, ReplyTarget};
use uuid::Uuid;

use crate::services::gateways::GatewayService;
use crate::services::gateways::concerned::shown_action;
use crate::services::gateways::telegram::{
    BotChat, Pause, WrittenMessage, after_failure, keep_message, reply_text, write_in_chat,
};

/// Writes the answer to `target`, of the company `tenant_id`, in the
/// person's private chat, with the bot's `token`; `awaiting`, a pause's
/// actions still awaiting a decision. What became of it. A pause written
/// with buttons is kept, to be written again once some of its actions are
/// settled (ADR-148).
pub(super) async fn answer(
    gateways: &GatewayService,
    (tenant_id, target): (Uuid, &ReplyTarget),
    awaiting: &[ApprovalRow],
    token: &str,
    now: DateTime<Utc>,
) -> ReplyRecord {
    // Their identifier, as the database holds it.
    let Ok(chat) = target.channel_id.parse::<i64>() else {
        tracing::warn!(gateway = %target.gateway_id, "Telegram chat unreadable");
        return ReplyRecord::Done("failed");
    };
    let url = gateways.conversation_url(target.conversation_id);
    let own: Vec<&ApprovalRow> = (awaiting.iter())
        .filter(|a| !a.routed_to_manager())
        .collect();
    let shown = shown_in_message(own.iter().map(|a| shown_action(a)));
    let pause = Pause {
        shown: &shown,
        in_application: own.len() - shown.len(),
        managed: awaiting.len() - own.len(),
    };
    let (status, said) = (target.status.as_str(), target.answer.as_deref());
    let keyboard = (target.status == "awaiting_approval" && !shown.is_empty())
        .then(|| approval_keyboard(&shown));
    let to = BotChat {
        gateway_id: target.gateway_id,
        token,
        chat,
    };
    let text = |markup| reply_text(markup, status, said, &url, pause);
    match write_in_chat(gateways, to, text, keyboard.as_ref()).await {
        Ok(number) => {
            if keyboard.is_some() {
                let written = WrittenMessage {
                    tenant_id,
                    gateway_id: target.gateway_id,
                    chat,
                    number,
                    place: CardPlace::Thread(target.thread_id),
                };
                let ids: Vec<Uuid> = shown.iter().map(|a| a.approval_id).collect();
                let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
                let others = (count(pause.in_application), count(pause.managed));
                keep_message(gateways, written, &ids, others).await;
            }
            ReplyRecord::Done("sent")
        }
        Err(failure) => after_failure(&failure, target.attempts + 1, now),
    }
}
