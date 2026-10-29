//! An end or pause answered in the person's private chat with the
//! company's Telegram bot (ADR-135), where its conversation is held, in the
//! HTML Telegram reads (`telegram::reply_text`); in plain text when
//! Telegram does not read it. A pause says where to validate: in the
//! application, which shows the exact parameters.

use chrono::{DateTime, Utc};
use mimorya_connectors::telegram::TelegramFailure;
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_replies::{ReplyRecord, ReplyTarget};

use super::later;
use crate::services::gateways::GatewayService;
use crate::services::gateways::telegram::{Markup, Pause, reply_text};

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
    let own = (awaiting.iter()).filter(|a| !a.routed_to_manager()).count();
    let pause = Pause {
        own,
        managed: awaiting.len() - own,
    };
    let (status, said) = (target.status.as_str(), target.answer.as_deref());
    let telegram = gateways.telegram();
    let html = reply_text(Markup::Html, status, said, &url, pause);
    let sent = match telegram.send_html(token, chat, &html).await {
        // Written as Telegram would not read it: said in plain text.
        Err(failure) if failure.markup_refused() => {
            tracing::warn!(gateway = %target.gateway_id, "Telegram refused the answer's markup");
            let plain = reply_text(Markup::Plain, status, said, &url, pause);
            telegram.send(token, chat, &plain).await
        }
        sent => sent,
    };
    match sent {
        Ok(_) => ReplyRecord::Done("sent"),
        Err(failure) => after_failure(&failure, target.attempts + 1, now),
    }
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
}
