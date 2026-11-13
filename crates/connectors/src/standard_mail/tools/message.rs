//! What the mail tools keep of an e-mail (ADR-205), read by mail-parser
//! from what the server sent: its headers decoded (encoded words, character
//! sets), its text (an HTML e-mail's converted to text), its attachments
//! named, never read. Every field is bounded: whoever wrote the e-mail, or
//! the server, decides what it holds.

use mail_parser::{Address, Message, MessageParser, MimeHeaders};
use mimorya_core::tools::Citation;
use serde_json::{Value, json};

use crate::delegated::shape::{MAX_SHORT_CHARS, bounded};

/// The most of an e-mail's text read.
pub const MAX_BODY_CHARS: usize = 20_000;
/// The most recipients and attachments listed.
const MAX_LISTED: usize = 20;

/// The headers `mail_search` fetches.
pub const SUMMARY_FIELDS: &str = "SUBJECT FROM TO DATE";

/// An e-mail parsed from what the server sent; `None` if nothing of it is
/// a message.
pub fn parsed(raw: &[u8]) -> Option<Message<'_>> {
    MessageParser::default().parse(raw)
}

/// Its headers parsed, alone.
pub fn headers(raw: &[u8]) -> Option<Message<'_>> {
    MessageParser::default().parse_headers(raw)
}

fn text(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |text| {
        json!(bounded(text.trim(), MAX_SHORT_CHARS).0)
    })
}

/// The people of an address header, the first ones: a name and an address
/// each, as written.
pub fn people(address: Option<&Address<'_>>) -> Value {
    let Some(address) = address else {
        return Value::Null;
    };
    let all: Vec<Value> = address
        .iter()
        .map(|person| {
            json!({
                "nom": text(person.name()),
                "adresse": text(person.address()),
            })
        })
        .collect();
    let more = all.len().saturating_sub(MAX_LISTED);
    let kept: Vec<Value> = all.into_iter().take(MAX_LISTED).collect();
    if more == 0 {
        json!(kept)
    } else {
        json!({ "personnes": kept, "autres": more })
    }
}

/// The subject, the sender, the recipients and the date the e-mail says.
pub fn summary(message: &Message<'_>) -> Value {
    json!({
        "objet": text(message.subject()),
        "de": people(message.from()),
        "a": people(message.to()),
        "date": message.date().map_or(Value::Null, |date| json!(date.to_rfc3339())),
    })
}

/// The e-mail's text, bounded, and whether it was cut.
pub fn body(message: &Message<'_>) -> (String, bool) {
    let text = message.body_text(0).unwrap_or_default();
    bounded(text.trim(), MAX_BODY_CHARS)
}

/// Its attachments: name, type and size, never their content.
pub fn attachments(message: &Message<'_>) -> Value {
    let all: Vec<Value> = message
        .attachments()
        .map(|part| {
            let kind = part.content_type().map(|kind| match kind.subtype() {
                Some(subtype) => format!("{}/{subtype}", kind.ctype()),
                None => kind.ctype().to_string(),
            });
            json!({
                "nom": text(part.attachment_name()),
                "type": text(kind.as_deref()),
                "octets": part.contents().len(),
            })
        })
        .collect();
    let more = all.len().saturating_sub(MAX_LISTED);
    let kept: Vec<Value> = all.into_iter().take(MAX_LISTED).collect();
    if more == 0 {
        json!(kept)
    } else {
        json!({ "fichiers": kept, "autres": more })
    }
}

/// The citation of the e-mail `id`, named by its subject.
pub fn citation(id: &str, message: Option<&Message<'_>>) -> Citation {
    let subject = message
        .and_then(Message::subject)
        .map(str::trim)
        .filter(|subject| !subject.is_empty())
        .unwrap_or("(sans objet)");
    Citation::new("email", id).labelled(bounded(subject, 120).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "From: =?UTF-8?Q?Ren=C3=A9e_Dupont?= <renee@exemple.fr>\r\n\
To: Paul <paul@neo-maintenance.example>, lea@exemple.fr\r\n\
Subject: =?ISO-8859-1?Q?R=E9union_de_chantier?=\r\n\
Date: Tue, 10 Nov 2026 09:30:00 +0100\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"b1\"\r\n\
\r\n\
--b1\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p>Bonjour,<br>le <b>chantier</b> commence lundi.</p>\r\n\
--b1\r\n\
Content-Type: application/pdf\r\n\
Content-Disposition: attachment; filename=\"devis.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0xLjQK\r\n\
--b1--\r\n";

    #[test]
    fn an_e_mail_is_read_decoded_its_attachments_named() {
        let message = parsed(RAW.as_bytes()).unwrap();
        let summary = summary(&message);
        assert_eq!(summary["objet"], "Réunion de chantier");
        assert_eq!(summary["de"][0]["nom"], "Renée Dupont");
        assert_eq!(summary["a"][1]["adresse"], "lea@exemple.fr");
        assert_eq!(summary["date"], "2026-11-10T09:30:00+01:00");
        let (text, cut) = body(&message);
        assert!(text.contains("chantier commence lundi"), "{text}");
        assert!(!text.contains("<b>") && !cut);
        let attached = attachments(&message);
        assert_eq!(attached[0]["nom"], "devis.pdf");
        assert_eq!(attached[0]["type"], "application/pdf");
        assert_eq!(attached[0]["octets"], 9);
    }

    #[test]
    fn what_an_e_mail_holds_is_bounded() {
        let long = format!(
            "Subject: {}\r\nTo: {}\r\n\r\n{}",
            "s".repeat(1_000),
            (0..30)
                .map(|n| format!("p{n}@exemple.fr"))
                .collect::<Vec<_>>()
                .join(", "),
            "t".repeat(MAX_BODY_CHARS + 10)
        );
        let message = parsed(long.as_bytes()).unwrap();
        let summary = summary(&message);
        assert_eq!(
            summary["objet"].as_str().unwrap().chars().count(),
            MAX_SHORT_CHARS
        );
        assert_eq!(summary["a"]["autres"], 10);
        let (text, cut) = body(&message);
        assert!(cut && text.chars().count() == MAX_BODY_CHARS);
        let citation = super::citation("r1.2", Some(&message));
        assert_eq!(citation.label.as_deref().map(str::len), Some(120));
        assert_eq!(
            super::citation("r1.2", None).label.as_deref(),
            Some("(sans objet)")
        );
    }
}
