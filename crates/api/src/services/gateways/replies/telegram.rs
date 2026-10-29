//! An end or pause answered in the person's private chat with the
//! company's Telegram bot (ADR-135), where its conversation is held, in the
//! HTML Telegram reads (`telegram::reply_text`); in plain text when
//! Telegram does not read it. A pause shows the actions awaiting the
//! person's own validation that may be given there with their exact
//! parameters and their buttons (ADR-136), as many as one message holds;
//! the others are validated in the application, which shows them whole.

use chrono::{DateTime, Utc};
use mimorya_connectors::telegram::TelegramFailure;
use mimorya_core::gateways::telegram::{approval_keyboard, shown_actions};
use mimorya_core::gateways::{ShownAction, validated_in_gateway};
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_replies::{ReplyRecord, ReplyTarget};

use super::later;
use crate::services::gateways::GatewayService;
use crate::services::gateways::telegram::{Markup, Pause, reply_text};

/// The most actions shown with their buttons in a pause's message.
pub const MAX_SHOWN_ACTIONS: usize = 5;
/// The most characters of shown actions in a message, which Telegram
/// takes up to 4 096 characters of text: room is left for the rest.
const MAX_SHOWN_CHARS: usize = 3_400;

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
    let shown = shown_here(&own);
    let pause = Pause {
        shown: &shown,
        in_application: own.len() - shown.len(),
        managed: awaiting.len() - own.len(),
    };
    let (status, said) = (target.status.as_str(), target.answer.as_deref());
    let keyboard = (target.status == "awaiting_approval" && !shown.is_empty())
        .then(|| approval_keyboard(&shown));
    let telegram = gateways.telegram();
    let write = |markup: Markup| {
        let text = reply_text(markup, status, said, &url, pause);
        let html = markup == Markup::Html;
        let keyboard = keyboard.as_ref();
        async move {
            match keyboard {
                Some(keyboard) => {
                    telegram
                        .send_buttons(token, chat, (&text, html), keyboard)
                        .await
                }
                None if html => telegram.send_html(token, chat, &text).await,
                None => telegram.send(token, chat, &text).await,
            }
        }
    };
    let sent = match write(Markup::Html).await {
        // Written as Telegram would not read it: said in plain text.
        Err(failure) if failure.markup_refused() => {
            tracing::warn!(gateway = %target.gateway_id, "Telegram refused the answer's markup");
            write(Markup::Plain).await
        }
        sent => sent,
    };
    match sent {
        Ok(_) => ReplyRecord::Done("sent"),
        Err(failure) => after_failure(&failure, target.attempts + 1, now),
    }
}

/// The person's `own` actions validated in Telegram, shown with their
/// buttons: those that may be validated there, as many as one message
/// holds, in order.
fn shown_here<'a>(own: &[&'a ApprovalRow]) -> Vec<ShownAction<'a>> {
    let mut shown = Vec::new();
    for approval in own {
        if !validated_in_gateway(&approval.risk, &approval.payload) {
            continue;
        }
        shown.push(ShownAction {
            approval_id: approval.id,
            summary: &approval.action_summary,
            risk: &approval.risk,
            parameters: &approval.payload,
        });
        let fits = shown_actions(&shown, false).chars().count() <= MAX_SHOWN_CHARS;
        if !fits || shown.len() > MAX_SHOWN_ACTIONS {
            shown.pop();
            break;
        }
    }
    shown
}

/// What becomes of an answer Telegram did not take, at its `attempt`-th
/// try: refused (the person blocked the bot, the token revoked…), never
/// tried again; unreached or failing on its side, tried again later, a few
/// times.
fn after_failure(failure: &TelegramFailure, attempt: i32, now: DateTime<Utc>) -> ReplyRecord {
    let reason = failure.message();
    tracing::warn!(attempt, %reason, "Telegram answer not taken");
    match failure {
        TelegramFailure::Refused { status, .. } if *status < 500 => ReplyRecord::Done("refused"),
        TelegramFailure::Throttled { retry_after } => later(attempt, *retry_after, now),
        _ => later(attempt, None, now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gateways::replies::MAX_ATTEMPTS;
    use chrono::Duration;
    use mimorya_connectors::telegram::RefusalCause;

    #[test]
    fn telegram_unreached_is_tried_again_later_and_refusing_never() {
        let now = Utc::now();
        let unreached = TelegramFailure::Unanswered("trop lent");
        assert_eq!(
            after_failure(&unreached, 1, now),
            ReplyRecord::Retry(now + Duration::seconds(30))
        );
        assert_eq!(
            after_failure(&TelegramFailure::Failed(502), MAX_ATTEMPTS, now),
            ReplyRecord::Done("failed")
        );
        let throttled = TelegramFailure::Throttled {
            retry_after: Some(std::time::Duration::from_secs(12)),
        };
        assert_eq!(
            after_failure(&throttled, 1, now),
            ReplyRecord::Retry(now + Duration::seconds(12))
        );
        let blocked = TelegramFailure::Refused {
            status: 403,
            cause: RefusalCause::Blocked,
        };
        assert_eq!(
            after_failure(&blocked, 1, now),
            ReplyRecord::Done("refused")
        );
        // An error on Telegram's side is no refusal.
        let failing = TelegramFailure::Refused {
            status: 500,
            cause: RefusalCause::Other,
        };
        assert_eq!(
            after_failure(&failing, 1, now),
            ReplyRecord::Retry(now + Duration::seconds(30))
        );
    }

    fn awaiting(payload: serde_json::Value, risk: &str) -> ApprovalRow {
        ApprovalRow {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            run_id: uuid::Uuid::new_v4(),
            requested_for_user_id: uuid::Uuid::new_v4(),
            tool_name: "planning_apply".into(),
            tool_call_id: "call-1".into(),
            action_summary: "Appliquer".into(),
            payload,
            risk: risk.into(),
            reason: String::new(),
            status: "pending".into(),
            decided_by_user_id: None,
            decided_at: None,
            decision_comment: None,
            expires_at: None,
            created_at: Utc::now(),
            routing: "requester".into(),
            approver_user_id: None,
        }
    }

    #[test]
    fn only_actions_validated_here_are_shown_as_many_as_a_message_holds() {
        let small = awaiting(serde_json::json!({ "proposalId": "p-1" }), "write");
        let external = awaiting(serde_json::json!({ "to": "x" }), "external");
        let large = awaiting(serde_json::json!({ "body": "x".repeat(2_000) }), "write");
        // Sent outside the company: in the application only.
        let shown = shown_here(&[&small, &external, &small]);
        assert_eq!(shown.len(), 2);
        // Two large ones do not fit one message: the first only.
        let shown = shown_here(&[&large, &large, &small]);
        assert_eq!(shown.len(), 1);
        // At most a few.
        let many: Vec<&ApprovalRow> = std::iter::repeat_n(&small, MAX_SHOWN_ACTIONS + 2).collect();
        assert_eq!(shown_here(&many).len(), MAX_SHOWN_ACTIONS);
    }
}
