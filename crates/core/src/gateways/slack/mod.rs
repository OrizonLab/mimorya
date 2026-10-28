//! The company's Slack application (ADR-125): what its administrators
//! declare of it (its bot's token, its signing secret), the permissions it
//! asks for and the events it subscribes to, how a request Slack sends
//! proves it comes from it (`signature`), which events reach the agent
//! (`events`), and when Slack proves a Slack user is a person (`identity`).

mod events;
mod identity;
mod signature;

use regex::Regex;
use std::sync::LazyLock;

pub use events::{
    DirectMessage, Envelope, MAX_MESSAGE_CHARS, direct_message, is_event, is_person, is_workspace,
};
pub use identity::{IdentityRefusal, SlackProfile, identity_refusal};
pub use signature::{MAX_SKEW_SECONDS, SignatureProblem, signature, verify};

/// The permissions (bot token scopes) the application asks for: write its
/// replies, read its direct conversations, and read who writes there with
/// their address, which proves the person (ADR-125). Nothing else: it reads
/// no channel, no file, no other conversation.
pub const BOT_SCOPES: [&str; 4] = ["chat:write", "im:history", "users:read", "users:read.email"];
/// The one event it subscribes to: a message in a direct conversation with it.
pub const BOT_EVENTS: [&str; 1] = ["message.im"];

/// A bot's token, as Slack issues it.
static BOT_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^xoxb-[A-Za-z0-9-]{10,250}$").expect("valid regex"));
/// A signing secret, as Slack shows it.
static SIGNING_SECRET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9]{16,128}$").expect("valid regex"));

/// What a company declares of its Slack application, checked.
#[derive(Clone, PartialEq, Eq)]
pub struct SlackApplication {
    pub bot_token: String,
    pub signing_secret: String,
}

/// Never its secrets, whatever prints it.
impl std::fmt::Debug for SlackApplication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackApplication")
            .field("bot_token", &"<redacted>")
            .field("signing_secret", &"<redacted>")
            .finish()
    }
}

/// The bot's token, if it reads as Slack issues them (`xoxb-…`), or why not
/// (in French).
pub fn bot_token(token: &str) -> Result<String, &'static str> {
    let token = token.trim();
    if BOT_TOKEN.is_match(token) {
        Ok(token.to_owned())
    } else {
        Err(
            "Jeton du bot : celui que Slack donne à l’application installée, commençant par \
             xoxb- (ni un jeton d’utilisateur xoxp-, ni un jeton d’application xapp-)",
        )
    }
}

/// The signing secret, if it reads as Slack shows them, or why not (in
/// French).
pub fn signing_secret(secret: &str) -> Result<String, &'static str> {
    let secret = secret.trim();
    if SIGNING_SECRET.is_match(secret) {
        Ok(secret.to_owned())
    } else {
        Err(
            "Secret de signature : celui que Slack montre dans « Basic Information » de \
             l’application (lettres et chiffres)",
        )
    }
}

/// The application a company declares, or why it cannot be one (in French).
pub fn slack_application(token: &str, secret: &str) -> Result<SlackApplication, &'static str> {
    Ok(SlackApplication {
        bot_token: bot_token(token)?,
        signing_secret: signing_secret(secret)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "xoxb-fictif-jeton-du-bot-de-test-0001";
    const SECRET: &str = "8f742231b10e8888abcd99eeefff85a5";

    #[test]
    fn an_application_never_prints_its_secrets() {
        let printed = format!("{:?}", slack_application(TOKEN, SECRET).unwrap());
        assert!(
            !printed.contains(TOKEN) && !printed.contains(SECRET),
            "{printed}"
        );
    }

    #[test]
    fn a_declared_application_is_trimmed() {
        let application =
            slack_application(&format!(" {TOKEN}\n"), &format!(" {SECRET} ")).unwrap();
        assert_eq!(application.bot_token, TOKEN);
        assert_eq!(application.signing_secret, SECRET);
    }

    #[test]
    fn only_a_bot_s_token_is_taken() {
        for token in [
            "xoxp-fictif-jeton-de-personne-0001",
            "xapp-1-A0123-0123456789-abcdef",
            "xoxb-",
            "xoxb-court",
            "Bearer xoxb-fictif-jeton-du-bot-de-test-0001",
            "xoxb-fictif-jeton du-bot",
            "xoxb-fictif-jeton-du-bot-de-test-000é",
        ] {
            assert!(slack_application(token, SECRET).is_err(), "{token}");
        }
    }

    #[test]
    fn only_a_signing_secret_as_slack_shows_it_is_taken() {
        for secret in [
            "",
            "court",
            "8f742231-b10e-8888",
            &"a".repeat(129),
            "8f742231b10e8888abcd99eeefff85a5é",
        ] {
            assert!(slack_application(TOKEN, secret).is_err(), "{secret}");
        }
    }
}
