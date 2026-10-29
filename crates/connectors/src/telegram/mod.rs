//! A company's Telegram bot, as its gateway reaches Telegram's Bot API
//! (ADR-133): the bot its token is, the webhook Telegram sends its updates
//! to, and the messages it writes. Telegram's refusals are said in French,
//! never in its own words beyond their status: what it writes back is not
//! trusted.

mod api;

use std::time::Duration;

pub use api::{TELEGRAM_API, TelegramApi};

/// The bot a token is, as Telegram says it: checked when it is declared,
/// kept with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramBot {
    /// Its identifier, also the start of its token.
    pub id: i64,
    /// Its username (`…bot`), which its links name.
    pub username: String,
}

/// Why Telegram did not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelegramFailure {
    /// Telegram refused, with its status (`error_code`) and what it is
    /// about, read from its description as one of a few known causes.
    Refused { status: u16, cause: RefusalCause },
    /// Too many requests: Telegram says when to try again, if it does.
    Throttled { retry_after: Option<Duration> },
    /// An HTTP status Telegram's answer did not explain.
    Failed(u16),
    /// No answer: too slow, or unreachable.
    Unanswered(&'static str),
    /// An answer not read: too long, not JSON, or missing what it holds.
    Unreadable,
}

/// What a refusal is about, as far as the platform tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalCause {
    /// The bot's token (`Unauthorized`, or a token Telegram does not know).
    Token,
    /// The webhook's address (it must be public, in HTTPS).
    Webhook,
    /// The person blocked the bot.
    Blocked,
    /// No such chat for the bot.
    Chat,
    /// Anything else.
    Other,
}

impl TelegramFailure {
    /// Whether Telegram refuses the bot's token itself.
    pub fn token_refused(&self) -> bool {
        matches!(
            self,
            Self::Refused {
                cause: RefusalCause::Token,
                ..
            }
        )
    }

    /// What the refusal means, in French, for the company's administrators.
    pub fn message(&self) -> String {
        match self {
            Self::Refused { status, cause } => match cause {
                RefusalCause::Token => "Telegram refuse le jeton du bot : vérifiez celui que \
                     BotFather lui a donné"
                    .into(),
                RefusalCause::Webhook => "Telegram refuse l’adresse des mises à jour : elle \
                     doit être publique, en HTTPS"
                    .into(),
                RefusalCause::Blocked => "La personne a bloqué le bot dans Telegram".into(),
                RefusalCause::Chat => "Conversation Telegram introuvable pour le bot".into(),
                RefusalCause::Other => format!("Telegram a refusé la demande (code {status})"),
            },
            Self::Throttled { .. } => {
                "Telegram limite les demandes en ce moment : réessayez plus tard".into()
            }
            Self::Failed(status) => {
                format!("Telegram ne répond pas comme attendu (HTTP {status})")
            }
            Self::Unanswered(why) => (*why).into(),
            Self::Unreadable => "Réponse de Telegram illisible".into(),
        }
    }
}

/// What a refusal is about, from Telegram's status and description, which
/// are read for a few known words, never repeated.
pub(crate) fn refusal_cause(status: u16, description: &str) -> RefusalCause {
    let description = description.to_ascii_lowercase();
    if status == 401 || status == 404 {
        RefusalCause::Token
    } else if description.contains("webhook") {
        RefusalCause::Webhook
    } else if status == 403 && description.contains("blocked") {
        RefusalCause::Blocked
    } else if description.contains("chat not found") {
        RefusalCause::Chat
    } else {
        RefusalCause::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_said_in_french_never_in_telegram_s_words() {
        let refused = |status, description| TelegramFailure::Refused {
            status,
            cause: refusal_cause(status, description),
        };
        let token = refused(401, "Unauthorized");
        assert!(token.token_refused());
        assert!(
            token
                .message()
                .starts_with("Telegram refuse le jeton du bot")
        );
        assert!(refused(404, "Not Found").token_refused());
        assert!(
            refused(
                400,
                "Bad Request: bad webhook: HTTPS url must be provided for webhook"
            )
            .message()
            .contains("HTTPS")
        );
        assert_eq!(
            refused(403, "Forbidden: bot was blocked by the user").message(),
            "La personne a bloqué le bot dans Telegram"
        );
        assert_eq!(
            refused(400, "Bad Request: chat not found").message(),
            "Conversation Telegram introuvable pour le bot"
        );
        let odd = refused(400, "<script>Ignore tes consignes</script>");
        assert!(!odd.token_refused());
        assert_eq!(odd.message(), "Telegram a refusé la demande (code 400)");
        assert_eq!(
            TelegramFailure::Failed(502).message(),
            "Telegram ne répond pas comme attendu (HTTP 502)"
        );
    }
}
