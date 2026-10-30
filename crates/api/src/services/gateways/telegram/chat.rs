//! What the bot writes, of itself, in a person's private chat: a run's end
//! or pause (ADR-135, ADR-136), the requests awaiting a manager (ADR-137).
//! In the HTML Telegram reads, with the buttons of the actions it shows
//! under it; in plain text once Telegram refuses the HTML. What Telegram
//! refuses is not tried again; Telegram unreachable or failing on its side,
//! tried again later, a few times.

use chrono::{DateTime, Utc};
use mimorya_connectors::telegram::TelegramFailure;
use mimorya_db::repositories::gateway_replies::ReplyRecord;
use serde_json::Value;
use uuid::Uuid;

use super::reply_text::Markup;
use crate::services::gateways::GatewayService;
use crate::services::gateways::retry::later;

/// A person's private chat with a company's bot.
#[derive(Debug, Clone, Copy)]
pub struct BotChat<'a> {
    pub gateway_id: Uuid,
    /// The bot's token.
    pub token: &'a str,
    /// The person's Telegram identifier, their private chat's.
    pub chat: i64,
}

/// Writes in `to` the text `text` makes in Telegram's HTML, with
/// `keyboard`'s buttons under it, if any; in plain text once Telegram
/// refuses the HTML. The message's number.
pub async fn write_in_chat(
    gateways: &GatewayService,
    to: BotChat<'_>,
    text: impl Fn(Markup) -> String,
    keyboard: Option<&Value>,
) -> Result<i64, TelegramFailure> {
    let telegram = gateways.telegram();
    let (token, chat) = (to.token, to.chat);
    let write = |markup: Markup| {
        let text = text(markup);
        let html = markup == Markup::Html;
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
    match write(Markup::Html).await {
        // Written as Telegram would not read it: said in plain text.
        Err(failure) if failure.markup_refused() => {
            tracing::warn!(gateway = %to.gateway_id, "Telegram refused the message's markup");
            write(Markup::Plain).await
        }
        sent => sent,
    }
}

/// What becomes of a message Telegram did not take, at its `attempt`-th
/// try: refused (the person blocked the bot, the token revoked…), never
/// tried again; unreached or failing on its side, tried again later, a few
/// times.
pub fn after_failure(failure: &TelegramFailure, attempt: i32, now: DateTime<Utc>) -> ReplyRecord {
    let reason = failure.message();
    tracing::warn!(attempt, %reason, "Telegram message not taken");
    match failure {
        TelegramFailure::Refused { status, .. } if *status < 500 => ReplyRecord::Done("refused"),
        TelegramFailure::Throttled { retry_after } => later(attempt, *retry_after, now),
        _ => later(attempt, None, now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gateways::retry::MAX_ATTEMPTS;
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
