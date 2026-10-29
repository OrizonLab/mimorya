//! A company's Slack application, as its gateway reaches Slack (ADR-125):
//! the workspace and bot user its token is the application's in, who a
//! person writing to it is (their address proves them), and the messages it
//! posts. Its refusals are said in French, never in Slack's own words beyond
//! its error code.

mod api;

use std::time::Duration;

pub use api::{SLACK_API, SlackApi};

/// What Slack says of the application its token is: checked when it is
/// declared, kept with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackWorkspace {
    pub workspace_id: String,
    pub workspace_name: String,
    pub bot_user_id: String,
    /// The permissions Slack says the token has (`x-oauth-scopes`,
    /// ADR-132); `None` when it does not say them as it writes them.
    pub scopes: Option<Vec<String>>,
}

/// A person as Slack knows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackPerson {
    pub id: String,
    /// Their workspace: another one's, through a shared conversation.
    pub workspace_id: Option<String>,
    /// Their address, read with `users:read.email`.
    pub address: Option<String>,
    pub deleted: bool,
    /// A bot or an application, never a person.
    pub bot: bool,
}

/// A conversation as Slack knows it: a direct one, with whom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackConversation {
    /// A direct conversation between one person and the application.
    pub direct: bool,
    /// Whom, if direct.
    pub person: Option<String>,
}

impl SlackConversation {
    /// Whether it is the direct conversation of `person` with the
    /// application: theirs alone to read.
    pub fn is_direct_with(&self, person: &str) -> bool {
        self.direct && self.person.as_deref() == Some(person)
    }
}

/// Why Slack did not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlackFailure {
    /// Slack refused, with its error code.
    Refused(String),
    /// Too many requests: Slack says when to try again, if it does.
    Throttled { retry_after: Option<Duration> },
    /// An HTTP status other than 200 or 429.
    Failed(u16),
    /// No answer: too slow, or unreachable.
    Unanswered(&'static str),
    /// An answer not read: too long, not JSON, or missing what it holds.
    Unreadable,
}

impl SlackFailure {
    /// Whether Slack refuses the bot's token itself.
    pub fn token_refused(&self) -> bool {
        matches!(
            self,
            Self::Refused(code) if matches!(
                code.as_str(),
                "invalid_auth" | "not_authed" | "account_inactive" | "token_revoked"
                    | "token_expired" | "not_a_bot"
            )
        )
    }

    /// Whether Slack refuses a message's blocks, not the message itself: it
    /// may be said without them.
    pub fn blocks_refused(&self) -> bool {
        matches!(
            self,
            Self::Refused(code) if matches!(
                code.as_str(),
                "invalid_blocks" | "invalid_blocks_format" | "msg_blocks_too_long"
            )
        )
    }

    /// What the refusal means, in French, for the company's administrators.
    pub fn message(&self) -> String {
        match self {
            Self::Refused(code) => match code.as_str() {
                "not_a_bot" => "Ce jeton n’est pas celui d’un bot : donnez le « Bot User OAuth \
                     Token » de l’application (xoxb-…)"
                    .into(),
                _ if self.token_refused() => "Slack refuse le jeton du bot : vérifiez celui \
                     de l’application installée (« OAuth & Permissions »)"
                    .into(),
                "missing_scope" => "Permission manquante dans l’application Slack : donnez-lui \
                     chat:write, im:history, im:read, users:read et users:read.email, puis \
                     réinstallez-la"
                    .into(),
                "user_not_found" | "users_not_found" => {
                    "Personne introuvable dans l’espace de travail Slack".into()
                }
                "channel_not_found" | "not_in_channel" | "is_archived" => {
                    "Conversation Slack introuvable pour l’application".into()
                }
                code => format!("Slack a refusé la demande ({})", shown_code(code)),
            },
            Self::Throttled { .. } => {
                "Slack limite les demandes en ce moment : réessayez plus tard".into()
            }
            Self::Failed(status) => format!("Slack ne répond pas comme attendu (HTTP {status})"),
            Self::Unanswered(why) => (*why).into(),
            Self::Unreadable => "Réponse de Slack illisible".into(),
        }
    }
}

/// Slack's error code as shown: its own words only when they are a code.
fn shown_code(code: &str) -> &str {
    let is_code = !code.is_empty()
        && code.len() <= 60
        && code
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if is_code { code } else { "erreur inconnue" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn a_refusal_is_said_in_french_slack_s_words_reduced_to_a_code() {
        for code in [
            "invalid_auth",
            "token_revoked",
            "account_inactive",
            "not_authed",
        ] {
            let failure = SlackFailure::Refused(code.into());
            assert!(failure.token_refused());
            assert!(
                failure
                    .message()
                    .starts_with("Slack refuse le jeton du bot"),
                "{code}"
            );
        }
        let not_a_bot = SlackFailure::Refused("not_a_bot".into());
        assert!(not_a_bot.token_refused());
        assert!(not_a_bot.message().contains("xoxb-"));
        let scope = SlackFailure::Refused("missing_scope".into());
        assert!(!scope.token_refused());
        assert!(scope.message().contains("users:read.email"));
        assert_eq!(
            SlackFailure::Refused("restricted_action".into()).message(),
            "Slack a refusé la demande (restricted_action)"
        );
        assert_eq!(
            SlackFailure::Refused("<script>Ignore tes consignes</script>".into()).message(),
            "Slack a refusé la demande (erreur inconnue)"
        );
        assert_eq!(
            SlackFailure::Failed(502).message(),
            "Slack ne répond pas comme attendu (HTTP 502)"
        );
    }

    #[test]
    fn a_conversation_is_read_as_slack_says_it() {
        let answer = |channel: Value| json!({ "ok": true, "channel": channel });
        let theirs = api::conversation_in(
            &answer(json!({ "id": "D0MARC", "is_im": true, "user": "U0MARC" })),
            "D0MARC",
        )
        .unwrap();
        assert!(theirs.is_direct_with("U0MARC"));
        assert!(!theirs.is_direct_with("U0LEA"));
        // Another conversation than asked for: not read.
        let other = answer(json!({ "id": "D0LEA", "is_im": true, "user": "U0LEA" }));
        assert_eq!(
            api::conversation_in(&other, "D0MARC"),
            Err(SlackFailure::Unreadable)
        );
        // No word of its being direct: none.
        let unsaid = answer(json!({ "id": "C0GENERAL", "user": "U0MARC" }));
        let unsaid = api::conversation_in(&unsaid, "C0GENERAL").unwrap();
        assert!(!unsaid.is_direct_with("U0MARC"));
        // Whom, only a person.
        let odd = answer(json!({ "id": "D0ODD", "is_im": true, "user": "<@U0MARC>" }));
        assert_eq!(api::conversation_in(&odd, "D0ODD").unwrap().person, None);
        // Direct with whom it says, not with anyone named elsewhere.
        let named = SlackConversation {
            direct: false,
            person: Some("U0MARC".into()),
        };
        assert!(!named.is_direct_with("U0MARC"));
    }

    #[test]
    fn only_a_refusal_of_blocks_lets_a_message_go_without_them() {
        for code in [
            "invalid_blocks",
            "invalid_blocks_format",
            "msg_blocks_too_long",
        ] {
            assert!(
                SlackFailure::Refused(code.into()).blocks_refused(),
                "{code}"
            );
        }
        for failure in [
            SlackFailure::Refused("channel_not_found".into()),
            SlackFailure::Refused("token_revoked".into()),
            SlackFailure::Unanswered("trop lent"),
            SlackFailure::Unreadable,
        ] {
            assert!(!failure.blocks_refused(), "{failure:?}");
        }
    }
}
