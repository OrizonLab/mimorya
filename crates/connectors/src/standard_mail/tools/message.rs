//! What the mail tools keep of an e-mail (ADR-205), read by mail-parser
//! from what the server sent: its headers decoded (encoded words, character
//! sets), its text (an HTML e-mail's converted to text), its attachments
//! named, never read. Every field is bounded: whoever wrote the e-mail, or
//! the server, decides what it holds.

use std::ops::Deref;

use mail_parser::{Address, Message, MessageParser, MimeHeaders, PartType};
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
pub fn parsed(raw: &[u8]) -> Option<Parsed<'_>> {
    MessageParser::default().parse(raw).map(Parsed)
}

/// An e-mail parsed by mail-parser, let go of without calling itself. The
/// library keeps each e-mail an e-mail encloses (`message/rfc822`) within
/// it, and dropping them one within another takes a stack frame each: a
/// sender's twenty thousand of them, in less than a megabyte, would
/// overflow the stack and end the process. They are taken out one by one.
pub struct Parsed<'x>(Message<'x>);

impl<'x> Deref for Parsed<'x> {
    type Target = Message<'x>;

    fn deref(&self) -> &Message<'x> {
        &self.0
    }
}

impl Drop for Parsed<'_> {
    fn drop(&mut self) {
        let mut enclosed = Vec::new();
        take_enclosed(&mut self.0, &mut enclosed);
        while let Some(mut message) = enclosed.pop() {
            take_enclosed(&mut message, &mut enclosed);
            // Dropped here, holding no e-mail any more.
        }
    }
}

/// Moves the e-mails `message` encloses into `into`.
fn take_enclosed<'x>(message: &mut Message<'x>, into: &mut Vec<Message<'x>>) {
    for part in &mut message.parts {
        if matches!(part.body, PartType::Message(_))
            && let PartType::Message(inner) = std::mem::take(&mut part.body)
        {
            into.push(inner);
        }
    }
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

    /// `count` e-mails, each enclosing the next, after the headers of
    /// the e-mail holding them.
    fn chain(count: usize) -> String {
        let mut raw = "Subject: Dedans\r\n".to_string();
        raw.push_str(&"Content-Type: message/rfc822\r\n\r\n".repeat(count));
        raw + "Fin\r\n"
    }

    /// How many e-mails deep a parsed e-mail encloses others, counted
    /// without calling itself.
    fn enclosed_depth(message: &Message<'_>) -> usize {
        let mut deepest = 0;
        let mut pending = vec![(message, 0)];
        while let Some((message, depth)) = pending.pop() {
            deepest = deepest.max(depth);
            for part in &message.parts {
                if let PartType::Message(inner) = &part.body {
                    pending.push((inner, depth + 1));
                }
            }
        }
        deepest
    }

    #[test]
    fn e_mails_enclosed_without_end_are_read_a_bounded_depth() {
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD;

        const HEAD: &str = "From: renee@exemple.fr\r\nSubject: Emboîtés\r\n";
        // As they are, quoted-printable (decoded, then read by the parser
        // calling itself), base64 within a multipart: thousands of e-mails
        // each enclosing the next, in less than a megabyte any sender may
        // send, would overflow the stack as they are read or let go of.
        let plain = format!(
            "{HEAD}Content-Type: message/rfc822\r\n\r\n{}",
            chain(30_000)
        );
        let quoted = format!(
            "{HEAD}Content-Type: message/rfc822\r\n\
             Content-Transfer-Encoding: quoted-printable\r\n\r\n{}",
            chain(30_000)
        );
        let encoded = STANDARD.encode(chain(20_000));
        let lines: Vec<&str> = encoded
            .as_bytes()
            .chunks(76)
            .map(|line| std::str::from_utf8(line).unwrap())
            .collect();
        let base64 = format!(
            "{HEAD}Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n--b\r\n\
             Content-Type: message/rfc822\r\nContent-Transfer-Encoding: base64\r\n\r\n\
             {}\r\n--b--\r\n",
            lines.join("\r\n")
        );
        // As they are, then decoded, at each level: every e-mail enclosing
        // the part counts, whatever its encoding.
        let mut mixed = chain(20);
        for as_they_are in [6, 6, 6] {
            mixed = format!(
                "Subject: Dedans\r\n{}Content-Type: message/rfc822\r\n\
                 Content-Transfer-Encoding: quoted-printable\r\n\r\n{mixed}",
                "Content-Type: message/rfc822\r\n\r\n".repeat(as_they_are)
            );
        }
        let mixed = format!("{HEAD}Content-Type: message/rfc822\r\n\r\n{mixed}");
        // Within a digest (a part without its type, read as an e-mail), as
        // `message/global`,
        // in base64: each form counts the same.
        let mut forms = chain(20);
        // A digest never holds a `message/global` directly: the parser's own
        // debug assertions refuse it, with or without the bound.
        let wraps = ["digest", "global", "base64"].repeat(2).into_iter();
        for (level, wrap) in wraps.chain(["digest", "global"]).enumerate() {
            forms = match wrap {
                "digest" => format!(
                    "Subject: Dedans\r\nContent-Type: multipart/digest; boundary=\"d{level}\"\r\n\r\n\
                     --d{level}\r\nContent-Description: joint\r\n\r\n{forms}\r\n--d{level}--\r\n"
                ),
                "global" => {
                    format!("Subject: Dedans\r\nContent-Type: message/global\r\n\r\n{forms}")
                }
                _ => format!(
                    "Subject: Dedans\r\nContent-Type: message/rfc822\r\n\
                     Content-Transfer-Encoding: base64\r\n\r\n{}",
                    STANDARD.encode(&forms)
                ),
            };
        }
        let forms = format!("{HEAD}Content-Type: message/rfc822\r\n\r\n{forms}");
        // Read 28 deep without the bound, the alternated forms may be cut a
        // level early: never past it.
        let reads = [(mixed, true), (forms, false), (quoted, true)];
        for (raw, exactly) in reads.into_iter().chain([(base64, true), (plain, true)]) {
            assert!(raw.len() < 1024 * 1024);
            let message = parsed(raw.as_bytes()).unwrap();
            assert_eq!(message.subject(), Some("Emboîtés"));
            // Past the bound, an enclosed e-mail is an attachment.
            let (bound, depth) = (
                mail_parser::nesting::MAX_NESTED_MESSAGES,
                enclosed_depth(&message),
            );
            match exactly {
                true => assert_eq!(depth, bound, "{:.120}", raw),
                false => assert!(
                    depth <= bound && depth + 1 >= bound,
                    "{depth}: {:.120}",
                    raw
                ),
            }
            drop(message);
        }
    }

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
