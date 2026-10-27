//! What a tool writes in a person's name, whichever provider sends it
//! (ADR-119, ADR-123): addresses in ASCII, a subject or a place on one
//! visible line, a text that shows all it holds (line breaks and tabs
//! aside). The person who validates sees all that is sent: a hidden
//! character could change what they read, and a letter of another script
//! could pass for one of theirs in an address, so both are refused before
//! they are asked, never taken out after.

use mimorya_core::plain_text::{is_hidden, shows_all_it_holds};
use validator::{ValidateEmail, ValidationError};

use super::shape::bounded;

/// The longest address accepted (RFC 5321's path, less its brackets).
const MAX_ADDRESS_CHARS: usize = 254;
/// The most of a text shown in the description of an action: the whole
/// text is in its parameters, shown to whoever validates it.
const EXCERPT_CHARS: usize = 300;

fn refused(code: &'static str, message: &'static str) -> ValidationError {
    ValidationError::new(code).with_message(message.into())
}

/// E-mail addresses, each once, in ASCII: `exеmple.fr` with a Cyrillic
/// `е` reads as `exemple.fr`.
pub fn addresses(list: &[String]) -> Result<(), ValidationError> {
    let invalid = list.iter().any(|address| {
        address.len() > MAX_ADDRESS_CHARS
            || !address.is_ascii()
            || address.chars().any(is_hidden)
            || !address.validate_email()
    });
    if invalid {
        return Err(refused(
            "addresses",
            "adresse électronique invalide (caractères ASCII seulement)",
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
        // What would end a header, or name another recipient, in a message.
        assert!(addresses(&list(&["lea@exemple.fr\r\nBcc: tous@exemple.fr"])).is_err());
        assert!(addresses(&list(&["\"lea, paul\"@exemple.fr"])).is_err());
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
