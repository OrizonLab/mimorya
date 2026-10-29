//! An end or pause answered in the person's private chat with the
//! company's Telegram bot (ADR-135), where its conversation is held, in the
//! HTML Telegram reads (`telegram::reply_text`); in plain text when
//! Telegram does not read it. A pause shows the actions awaiting the
//! person's own validation that may be given there with their exact
//! parameters and their buttons (ADR-136), as many as one message holds
//! (`shown_in_message`); the others are validated in the application,
//! which shows them whole.

use chrono::{DateTime, Utc};
use mimorya_core::gateways::ShownAction;
use mimorya_core::gateways::telegram::{approval_keyboard, shown_in_message};
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_replies::{ReplyRecord, ReplyTarget};

use crate::services::gateways::GatewayService;
use crate::services::gateways::telegram::{
    BotChat, Pause, after_failure, reply_text, write_in_chat,
};

/// Writes the answer to `target` in the person's private chat, with the
/// bot's `token`; `awaiting`, a pause's actions still awaiting a decision.
/// What became of it.
pub(super) async fn answer(
    gateways: &GatewayService,
    target: &ReplyTarget,
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
    let shown = shown_in_message(own.iter().map(|a| ShownAction {
        approval_id: a.id,
        summary: &a.action_summary,
        risk: &a.risk,
        parameters: &a.payload,
    }));
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
        Ok(_) => ReplyRecord::Done("sent"),
        Err(failure) => after_failure(&failure, target.attempts + 1, now),
    }
}
