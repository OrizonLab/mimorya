//! The text of an e-mail sent in a person's name (ADR-119, ADR-123,
//! ADR-205), written as RFC 5322 asks: the headers its sender adds first
//! (those the provider does not add itself), then its recipients, plain
//! addresses only, its subject in encoded words (RFC 2047) and its text in
//! UTF-8, both in base 64, so that nothing the model wrote can end a header
//! or add one, whatever the validation let through.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::{NewMail, is_plain_address};

/// The bytes of one encoded word: 39 encode to 52 characters, a word of 64
/// with its `=?UTF-8?B?` and `?=`; after `Subject: `, its line keeps
/// within the 76 characters RFC 2047 allows a line of encoded words.
const WORD_BYTES: usize = 39;
/// The characters of a line of the text in base 64 (RFC 2045).
const LINE_CHARS: usize = 76;

/// The text of `mail`, the `sender`'s headers first, written by the
/// platform; none when an address is not a plain one (the validation
/// refuses it first), or when a header the platform gives would hold a line
/// break or a control character.
pub fn message_text(mail: &NewMail, sender: &[(&str, &str)]) -> Option<String> {
    let all = mail.to.iter().chain(&mail.cc);
    if all.clone().any(|address| !is_plain_address(address)) {
        return None;
    }
    let mut message = String::new();
    for (name, value) in sender {
        if value.chars().any(char::is_control) {
            return None;
        }
        message.push_str(&format!("{name}: {value}\r\n"));
    }
    message.push_str(&format!("To: {}\r\n", folded(&mail.to)));
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
    Some(message)
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

    fn mail() -> NewMail {
        serde_json::from_value(json!({
            "to": ["paul@exemple.fr"],
            "subject": "Point",
            "body": "Bonjour",
        }))
        .unwrap()
    }

    #[test]
    fn the_sender_s_headers_come_first_and_hide_nothing() {
        let sender = [
            ("From", "lea@exemple.fr"),
            ("Date", "Fri, 13 Nov 2026 09:00:00 +0000"),
        ];
        let text = message_text(&mail(), &sender).unwrap();
        assert!(text.starts_with("From: lea@exemple.fr\r\nDate: Fri, 13 Nov 2026"));
        assert!(text.contains("\r\nTo: paul@exemple.fr\r\n"));
        // A header the platform gives never holds a line break.
        let forged = [("From", "lea@exemple.fr\r\nBcc: tous@exemple.fr")];
        assert_eq!(message_text(&mail(), &forged), None);
    }
}
