//! What a tool writes in a person's name, whichever provider sends it
//! (ADR-119, ADR-123): plain addresses in ASCII, a subject or a place on
//! one visible line, a text that shows all it holds (line breaks and tabs
//! aside). The person who validates sees all that is sent: a hidden
//! character could change what they read, a letter of another script could
//! pass for one of theirs in an address, and what a header reads as a
//! separator or a comment could change whom it is sent to, so all are
//! refused before they are asked, never taken out after.

use std::sync::LazyLock;

use mimorya_core::plain_text::{is_hidden, shows_all_it_holds};
use regex::Regex;
use validator::ValidationError;

use super::shape::bounded;

/// The longest address accepted (RFC 5321's path, less its brackets).
const MAX_ADDRESS_CHARS: usize = 254;
/// A plain address: before its one `@`, the characters HTML5 allows (those
/// of an RFC 5322 atom, dots anywhere: no space, comma, bracket, quote or
/// colon), 64 at most; after it, a host name of two labels or more. No IP
/// literal, no quoted or commented part: nothing a header reads as anything
/// but one address. A dot misplaced (`.lea@…`) is not refused here: it
/// changes no header, and the provider accepts or refuses the address.
static PLAIN_ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]{1,64}@[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)+$",
    )
    .expect("valid regex")
});
/// The most of a text shown in the description of an action: the whole
/// text is in its parameters, shown to whoever validates it.
const EXCERPT_CHARS: usize = 300;

fn refused(code: &'static str, message: &'static str) -> ValidationError {
    ValidationError::new(code).with_message(message.into())
}

/// Whether this is one plain address, in ASCII: `exеmple.fr` with a
/// Cyrillic `е` reads as `exemple.fr`.
pub fn is_plain_address(address: &str) -> bool {
    address.len() <= MAX_ADDRESS_CHARS && PLAIN_ADDRESS.is_match(address)
}

/// E-mail addresses, each plain and given once.
pub fn addresses(list: &[String]) -> Result<(), ValidationError> {
    let invalid = list.iter().any(|address| !is_plain_address(address));
    if invalid {
        return Err(refused(
            "addresses",
            "adresse électronique simple attendue (nom@domaine.fr, en caractères ASCII)",
        ));
    }
    let repeated = list.iter().enumerate().any(|(at, address)| {
        list[..at]
            .iter()
            .any(|before| before.eq_ignore_ascii_case(address))
    });
    if repeated {
        return Err(refused("addresses", "adresse électronique en double"));
    }
    Ok(())
}

/// One line, all of it visible, not blank.
pub fn visible_line(text: &str) -> Result<(), ValidationError> {
    if text.trim().is_empty() || text.chars().any(is_hidden) {
        return Err(refused(
            "visible_line",
            "une ligne de texte visible, sans caractère caché",
        ));
    }
    Ok(())
}

/// A text showing all it holds, not blank.
pub fn full_text(text: &str) -> Result<(), ValidationError> {
    if text.trim().is_empty() || !shows_all_it_holds(text) {
        return Err(refused(
            "full_text",
            "un texte visible, sans caractère caché (retours à la ligne et tabulations permis)",
        ));
    }
    Ok(())
}

/// The start of a text, for the description of an action.
pub fn excerpt(text: &str) -> String {
    match bounded(text.trim(), EXCERPT_CHARS) {
        (start, true) => format!("{start}…"),
        (whole, false) => whole,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(addresses: &[&str]) -> Vec<String> {
        addresses.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn addresses_are_valid_and_given_once() {
        assert!(addresses(&list(&["lea@exemple.fr", "paul@exemple.fr"])).is_ok());
        assert!(addresses(&list(&["lea@exemple.fr", "LEA@exemple.fr"])).is_err());
        assert!(addresses(&list(&["pas une adresse"])).is_err());
        assert!(addresses(&list(&["lea\u{200B}@exemple.fr"])).is_err());
        // Another script's letter, or a full-width one, passes for ours.
        assert!(addresses(&list(&["paul@ex\u{435}mple.fr"])).is_err());
        assert!(addresses(&list(&["paul@\u{FF45}xemple.fr"])).is_err());
        // What would end a header, or name another recipient, in a message,
        // with one `@` only (an IP literal ends the domain).
        for header in [
            "lea@exemple.fr\r\nBcc: tous@exemple.fr",
            "\"lea, paul\"@exemple.fr",
            "lea@exemple.fr, Direction (y[1.2.3.4]",
            "lea@exemple.fr (x[1.2.3.4]",
            "lea@exemple.fr\r\nBcc: x[1.2.3.4]",
            "Direction:;lea@exemple.fr",
            "lea@[1.2.3.4]",
            "lea@localhost",
            "lea@-exemple.fr",
        ] {
            assert!(addresses(&list(&[header])).is_err(), "{header}");
        }
        assert!(addresses(&list(&["l.martin+chantier@xn--exmple-cva.fr"])).is_ok());
        let long = format!("{}@exemple.fr", "a".repeat(250));
        assert!(addresses(&[long]).is_err());
    }

    #[test]
    fn what_is_sent_hides_nothing() {
        assert!(visible_line("Réunion de chantier").is_ok());
        assert!(visible_line("Objet\nsur deux lignes").is_err());
        assert!(visible_line("Objet \u{202E}inversé").is_err());
        assert!(visible_line("   ").is_err());
        assert!(full_text("Bonjour,\n\n\tà demain.").is_ok());
        assert!(full_text("Bonjour\u{200B}").is_err());
        assert!(full_text("Bonjour\r\n").is_err());
    }

    #[test]
    fn a_long_text_is_described_by_its_start() {
        assert_eq!(excerpt("  court  "), "court");
        let long = "é".repeat(EXCERPT_CHARS + 1);
        assert_eq!(excerpt(&long).chars().count(), EXCERPT_CHARS + 1);
        assert!(excerpt(&long).ends_with('…'));
    }
}
