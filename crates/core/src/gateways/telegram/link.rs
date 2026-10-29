//! The code a person links their Telegram account with (ADR-133): made in
//! their own session, kept as its digest only, carried to Telegram in the
//! bot's link (`https://t.me/<bot>?start=<code>`, which Telegram sends back
//! as `/start <code>` once the person starts the bot). Whoever opens it is
//! told whose account it links, and only proposes a link by accepting it
//! (`/oui`); the session that asked for it then confirms.

use regex::Regex;
use ring::digest;
use std::sync::LazyLock;

/// How long a code is: 32 random bytes, in unpadded base64url (Telegram
/// takes up to 64 of these characters after `start=`).
pub const LINK_CODE_CHARS: usize = 43;

/// A code, as the platform writes them.
static LINK_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{43}$").expect("valid regex"));
/// A bot's username, as Telegram requires them: ending in `bot`.
static BOT_USERNAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]{1,29}[Bb][Oo][Tt]$").expect("valid regex"));

/// Whether `code` is a code as the platform writes them.
pub fn is_link_code(code: &str) -> bool {
    LINK_CODE.is_match(code)
}

/// Whether `username` is a bot's username as Telegram writes them.
pub fn is_bot_username(username: &str) -> bool {
    BOT_USERNAME.is_match(username)
}

/// What is kept of a code: its digest, in hexadecimal.
pub fn link_code_hash(code: &str) -> String {
    let digest = digest::digest(&digest::SHA256, code.as_bytes());
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// The link that starts the bot `username` with `code`.
pub fn deep_link(username: &str, code: &str) -> String {
    format!("https://t.me/{username}?start={code}")
}

/// What a person sends in Telegram to accept the link they were offered.
pub const CONSENT: &str = "/oui";

/// Whether a message accepts the link its writer was offered.
pub fn is_consent(text: &str) -> bool {
    text.trim().eq_ignore_ascii_case(CONSENT)
}

/// The code a message carries, if it is the `/start` Telegram sends when
/// a person starts the bot from its link: `Some(None)` for `/start` alone
/// (or with anything but a code), `None` for any other message.
pub fn start_code(text: &str) -> Option<Option<&str>> {
    let rest = text.trim().strip_prefix("/start")?;
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    let code = rest.trim();
    Some(is_link_code(code).then_some(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-abcd";

    #[test]
    fn a_start_names_its_code_if_it_carries_one() {
        let code = format!("{CODE}e");
        assert_eq!(code.len(), LINK_CODE_CHARS);
        assert_eq!(
            start_code(&format!("/start {code}")),
            Some(Some(code.as_str()))
        );
        assert_eq!(start_code("/start"), Some(None));
        assert_eq!(start_code("/start court"), Some(None));
        assert_eq!(start_code(&format!("/start {code}x")), Some(None));
        assert_eq!(start_code("/started"), None);
        assert_eq!(start_code("Bonjour"), None);
        assert_eq!(start_code(&format!("Bonjour /start {code}")), None);
    }

    #[test]
    fn a_link_is_accepted_by_its_own_command_only() {
        for accepted in ["/oui", " /OUI ", "/Oui"] {
            assert!(is_consent(accepted), "{accepted}");
        }
        for other in ["oui", "/ouix", "/oui merci", "/start", ""] {
            assert!(!is_consent(other), "{other}");
        }
    }

    #[test]
    fn a_code_is_kept_as_its_digest_and_carried_by_the_bot_s_link() {
        let hash = link_code_hash("abc");
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(link_code_hash("abd"), hash);
        assert_eq!(
            deep_link("NeoMaintenanceBot", "xyz"),
            "https://t.me/NeoMaintenanceBot?start=xyz"
        );
    }

    #[test]
    fn a_bot_s_username_ends_in_bot() {
        for username in ["NeoMaintenanceBot", "neo_bot", "abBOT"] {
            assert!(is_bot_username(username), "{username}");
        }
        for username in [
            "NeoMaintenance",
            "bot",
            "1neo_bot",
            "neo-bot",
            "@neo_bot",
            "",
        ] {
            assert!(!is_bot_username(username), "{username}");
        }
        assert!(!is_bot_username(&format!("a{}bot", "b".repeat(30))));
    }
}
