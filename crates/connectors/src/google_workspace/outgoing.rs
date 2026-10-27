//! The e-mail Gmail sends (ADR-123), written as RFC 5322 asks: its
//! recipients, its subject in encoded words (RFC 2047) and its text in
//! UTF-8, both in base 64, so that nothing the model wrote can end a header
//! or add one; then the whole in URL-safe base 64, as Gmail reads it
//! (`raw`). Gmail adds the sender (the person whose token sends it), the
//! date and the message's identifier.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};

use crate::delegated::NewMail;

/// The bytes of one encoded word: 39 encode to 52 characters, a word of 64
/// with its `=?UTF-8?B?` and `?=`; after `Subject: `, its line keeps
/// within the 76 characters RFC 2047 allows a line of encoded words.
const WORD_BYTES: usize = 39;
/// The characters of a line of the text in base 64 (RFC 2045).
const LINE_CHARS: usize = 76;

/// The message, as Gmail's `raw`.
pub fn raw_message(mail: &NewMail) -> String {
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
    URL_SAFE.encode(message)
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
        String::from_utf8(URL_SAFE.decode(raw_message(mail)).unwrap()).unwrap()
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
