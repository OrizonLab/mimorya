//! The e-mail Gmail sends (ADR-123), written as RFC 5322 asks: its
//! recipients, plain addresses only, its subject in encoded words
//! (RFC 2047) and its text in UTF-8, both in base 64, so that nothing the
//! model wrote can end a header or add one, whatever the validation let
//! through; then the whole in URL-safe base 64, as Gmail reads it (`raw`).
//! Gmail adds the sender (the person whose token sends it), the date and
//! the message's identifier.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};

use crate::delegated::{NewMail, is_plain_address};

/// The bytes of one encoded word: 39 encode to 52 characters, a word of 64
/// with its `=?UTF-8?B?` and `?=`; after `Subject: `, its line keeps
/// within the 76 characters RFC 2047 allows a line of encoded words.
const WORD_BYTES: usize = 39;
/// The characters of a line of the text in base 64 (RFC 2045).
const LINE_CHARS: usize = 76;

/// The message, as Gmail's `raw`; none when an address is not a plain one
/// (the validation refuses it first).
pub fn raw_message(mail: &NewMail) -> Option<String> {
    let all = mail.to.iter().chain(&mail.cc);
    if all.clone().any(|address| !is_plain_address(address)) {
        return None;
    }
    let mut message = format!("To: {}\r\n", folded(&mail.to));
    if !mail.cc.is_empty() {
        message.push_str(&format!("Cc: {}\r\n", folded(&mail.cc)));
    }
    message.push_str(&format!("Subject: {}\r\n", encoded_words(&mail.subject)));
    message.push_str("MIME-Version: 1.0\r\n");
    message.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
    message.push_str("Content-Transfer-Encoding: base64\r\n\r\n");
    let text = STANDARD.encode(mail.body.replace('\n', "\r\n"));
    for line in text.as_bytes().chunks(LINE_CHARS) {
        message.push_str(&String::from_utf8_lossy(line));
        message.push_str("\r\n");
    }
    Some(URL_SAFE.encode(message))
}

/// Addresses one per line, so that ten long ones keep each line short.
fn folded(addresses: &[String]) -> String {
    addresses.join(",\r\n ")
}

/// A text as encoded words, one per line, each cut between characters:
/// readers join them back whole (RFC 2047, 6.2).
fn encoded_words(text: &str) -> String {
    let mut words = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if word.len() + c.len_utf8() > WORD_BYTES {
            words.push(std::mem::take(&mut word));
        }
        word.push(c);
    }
    words.push(word);
    words
        .iter()
        .map(|word| format!("=?UTF-8?B?{}?=", STANDARD.encode(word)))
        .collect::<Vec<_>>()
        .join("\r\n ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn mail(subject: &str, cc: &[&str]) -> NewMail {
        serde_json::from_value(json!({
            "to": ["paul@exemple.fr", "marc@exemple.fr"],
            "cc": cc,
            "subject": subject,
            "body": "Bonjour,\n\n\tLe planning est prêt.",
        }))
        .unwrap()
    }

    fn message(mail: &NewMail) -> String {
        String::from_utf8(URL_SAFE.decode(raw_message(mail).unwrap()).unwrap()).unwrap()
    }

    /// Read as the model gave it, not validated.
    fn unchecked(to: &str, subject: &str, body: &str) -> NewMail {
        serde_json::from_value(json!({ "to": [to], "subject": subject, "body": body })).unwrap()
    }

    /// The text of encoded words, joined back.
    fn decoded(words: &str) -> String {
        let mut bytes = Vec::new();
        for word in words.split("\r\n ") {
            let inner = word
                .strip_prefix("=?UTF-8?B?")
                .and_then(|w| w.strip_suffix("?="))
                .unwrap();
            bytes.extend(STANDARD.decode(inner).unwrap());
        }
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn a_message_holds_what_was_validated() {
        let message = message(&mail("Planning de novembre", &["claire@exemple.fr"]));
        let (head, text) = message.split_once("\r\n\r\n").unwrap();
        assert_eq!(
            head,
            "To: paul@exemple.fr,\r\n marc@exemple.fr\r\n\
             Cc: claire@exemple.fr\r\n\
             Subject: =?UTF-8?B?UGxhbm5pbmcgZGUgbm92ZW1icmU=?=\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: text/plain; charset=\"UTF-8\"\r\n\
             Content-Transfer-Encoding: base64"
        );
        let text = STANDARD.decode(text.replace("\r\n", "")).unwrap();
        assert_eq!(
            String::from_utf8(text).unwrap(),
            "Bonjour,\r\n\r\n\tLe planning est prêt."
        );
        // Without a copy, no `Cc` header.
        assert!(!self::message(&mail("Point", &[])).contains("Cc:"));
    }

    #[test]
    fn what_the_model_wrote_adds_no_header_even_unvalidated() {
        let injected = unchecked(
            "paul@exemple.fr",
            "Objet\r\nBcc: tous@exemple.fr",
            "Bonjour\r\nBcc: tous@exemple.fr\r\n\r\nFin",
        );
        let message = message(&injected);
        let (head, _) = message.split_once("\r\n\r\n").unwrap();
        let names: Vec<&str> = head
            .split("\r\n")
            .filter(|line| !line.starts_with(' '))
            .filter_map(|line| line.split_once(':').map(|(name, _)| name))
            .collect();
        assert_eq!(
            names,
            [
                "To",
                "Subject",
                "MIME-Version",
                "Content-Type",
                "Content-Transfer-Encoding"
            ]
        );
        assert!(!message.contains("Bcc"));
        // An address that is not a plain one: no message at all.
        for address in [
            "paul@exemple.fr, x[1.2.3.4]",
            "paul@exemple.fr\r\nBcc: x[1.2.3.4]",
        ] {
            assert_eq!(
                raw_message(&unchecked(address, "Objet", "Texte")),
                None,
                "{address}"
            );
        }
    }

    #[test]
    fn a_long_subject_is_cut_in_words_read_back_whole() {
        let subject = "Réunion d’équipe — préparation du chantier ".repeat(6);
        let message = message(&mail(&subject, &[]));
        let header = message
            .split("\r\n")
            .skip_while(|line| !line.starts_with("Subject: "))
            .take_while(|line| line.starts_with("Subject: ") || line.starts_with(' '))
            .collect::<Vec<_>>()
            .join("\r\n");
        let words = header.strip_prefix("Subject: ").unwrap();
        assert_eq!(decoded(words), subject);
        // Each word and each line within RFC 2047's bounds.
        assert!(words.split("\r\n ").count() > 1);
        assert!(words.split("\r\n ").all(|word| word.len() <= 75));
        assert!(message.split("\r\n").all(|line| line.len() <= 76));
    }
}
