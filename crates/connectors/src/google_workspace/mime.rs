//! What an e-mail Gmail gives in full (`format=full`) says (ADR-122): its
//! headers (Gmail decodes them), its text (the plain text part, else the
//! HTML one as text) in the charset it declares, and the names of its
//! attachments, which are not read. Gmail gives each part's content in
//! URL-safe base 64. The parts walked are bounded in depth and in number.

use std::sync::LazyLock;

use base64::Engine;
use base64::alphabet::URL_SAFE;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use encoding_rs::{Encoding, REPLACEMENT, UTF_8};
use regex::Regex;
use serde_json::Value;

use super::text::{html_to_text, tidy};
use crate::delegated::shape::bounded;

/// Gmail's base 64, padded or not.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);
const MAX_DEPTH: usize = 12;
const MAX_PARTS: usize = 200;
/// The attachments named, at most; the others counted.
pub const MAX_ATTACHMENTS: usize = 20;
const MAX_FILENAME_CHARS: usize = 200;

static CHARSET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)charset\s*=\s*"?([A-Za-z0-9._:-]{1,40})"#).expect("valid regex")
});

/// What an e-mail says, once read.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Mail {
    pub text: String,
    pub attachments: Vec<String>,
    /// Attachments past those named.
    pub more_attachments: usize,
}

/// A header of a part (`payload.headers`), its name in any case.
pub fn header<'a>(part: &'a Value, name: &str) -> Option<&'a str> {
    part.get("headers")?
        .as_array()?
        .iter()
        .find(|h| {
            h.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })?
        .get("value")?
        .as_str()
}

/// The e-mail of this payload: its text and its attachments.
pub fn read(payload: &Value) -> Mail {
    let mut walk = Walk::default();
    walk.visit(payload, 0);
    let text = match (walk.plain, walk.html) {
        (Some(part), _) => content(part).map(|text| tidy(&text)),
        (None, Some(part)) => content(part).map(|html| html_to_text(&html)),
        (None, None) => None,
    };
    let more_attachments = walk.attachments.len().saturating_sub(MAX_ATTACHMENTS);
    walk.attachments.truncate(MAX_ATTACHMENTS);
    Mail {
        text: text.unwrap_or_default(),
        attachments: walk.attachments,
        more_attachments,
    }
}

#[derive(Default)]
struct Walk<'a> {
    plain: Option<&'a Value>,
    html: Option<&'a Value>,
    attachments: Vec<String>,
    seen: usize,
}

impl<'a> Walk<'a> {
    fn visit(&mut self, part: &'a Value, depth: usize) {
        if depth > MAX_DEPTH || self.seen >= MAX_PARTS {
            return;
        }
        self.seen += 1;
        // A named part is an attachment, whatever its type: not read.
        let filename = part
            .get("filename")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !filename.is_empty() {
            self.attachments
                .push(bounded(filename, MAX_FILENAME_CHARS).0);
            return;
        }
        if let Some(parts) = part.get("parts").and_then(Value::as_array) {
            for child in parts {
                self.visit(child, depth + 1);
            }
            return;
        }
        let mime = part
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if mime.eq_ignore_ascii_case("text/plain") && self.plain.is_none() {
            self.plain = Some(part);
        } else if mime.eq_ignore_ascii_case("text/html") && self.html.is_none() {
            self.html = Some(part);
        }
    }
}

/// A part's content, in the charset its type declares (UTF-8 otherwise,
/// and for those the WHATWG reads as a single replacement character).
fn content(part: &Value) -> Option<String> {
    let data = part.pointer("/body/data").and_then(Value::as_str)?;
    let bytes = BASE64.decode(data).ok()?;
    let encoding = header(part, "Content-Type")
        .and_then(|value| CHARSET.captures(value))
        .and_then(|found| Encoding::for_label(found[1].as_bytes()))
        .filter(|encoding| *encoding != REPLACEMENT)
        .unwrap_or(UTF_8);
    Some(encoding.decode(&bytes).0.into_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn part(mime: &str, charset: &str, bytes: &[u8]) -> Value {
        json!({
            "mimeType": mime,
            "filename": "",
            "headers": [{ "name": "Content-Type", "value": format!("{mime}; charset=\"{charset}\"") }],
            "body": { "data": BASE64.encode(bytes) },
        })
    }

    #[test]
    fn the_plain_text_is_read_in_its_charset() {
        // "Préparé" in ISO-8859-1.
        let latin = part("text/plain", "ISO-8859-1", b"Pr\xe9par\xe9\r\n\r\n\r\nFin");
        let html = part("text/html", "utf-8", "<p>autre</p>".as_bytes());
        let attachment = json!({ "mimeType": "application/pdf", "filename": "devis.pdf" });
        let payload = json!({
            "mimeType": "multipart/mixed",
            "headers": [{ "name": "subject", "value": "Objet" }],
            "parts": [
                { "mimeType": "multipart/alternative", "filename": "", "parts": [latin, html] },
                attachment,
            ],
        });
        assert_eq!(header(&payload, "Subject"), Some("Objet"));
        assert_eq!(
            read(&payload),
            Mail {
                text: "Préparé\n\nFin".into(),
                attachments: vec!["devis.pdf".into()],
                more_attachments: 0,
            }
        );
    }

    #[test]
    fn an_html_only_mail_reads_as_its_text() {
        let html = part("text/html", "utf-8", "<p>Bonjour&nbsp;Léa</p>".as_bytes());
        assert_eq!(read(&html).text, "Bonjour Léa");
        // Unknown charset, or one the WHATWG replaces whole: UTF-8.
        for charset in ["x-inconnu", "iso-2022-kr", "hz-gb-2312"] {
            let odd = part("text/plain", charset, "été".as_bytes());
            assert_eq!(read(&odd).text, "été", "{charset}");
        }
    }

    #[test]
    fn attachments_are_named_up_to_a_bound_and_never_read() {
        let named: Vec<Value> = (0..25)
            .map(|i| json!({ "mimeType": "text/plain", "filename": format!("f{i}.txt"), "body": { "data": BASE64.encode(b"secret") } }))
            .collect();
        let mail = read(&json!({ "mimeType": "multipart/mixed", "parts": named }));
        assert_eq!(mail.text, "");
        assert_eq!(mail.attachments.len(), MAX_ATTACHMENTS);
        assert_eq!(mail.more_attachments, 5);
    }

    #[test]
    fn a_deep_tree_is_walked_only_so_far() {
        let mut nested = part("text/plain", "utf-8", b"profond");
        for _ in 0..(MAX_DEPTH + 2) {
            nested = json!({ "mimeType": "multipart/mixed", "parts": [nested] });
        }
        assert_eq!(read(&nested).text, "");
        // Unreadable base 64: no text.
        let broken = json!({ "mimeType": "text/plain", "body": { "data": "%%%" } });
        assert_eq!(read(&broken).text, "");
    }
}
