//! What an API answered, as the model reads it (ADR-112): its HTTP status
//! and its body, read up to a bound, as JSON when it says so and holds it
//! whole, as text when it is text, otherwise named but not read. The token
//! is masked before the body is cut, and what follows the cut is dropped far
//! enough back that no part of the token can remain. Headers are not passed
//! on (a cookie, a session).

use reqwest_mcp::Response;
use reqwest_mcp::header::CONTENT_TYPE;
use serde_json::{Value, json};

use crate::outbound::Redactor;

/// The most of a body the model reads.
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// An answer, read up to the bound and a little more (the token's longest
/// spelling), so that masking sees whole any token the bound cuts.
pub struct Answer {
    pub status: u16,
    content_type: Option<String>,
    body: Vec<u8>,
    /// More was sent than `MAX_RESPONSE_BYTES`.
    truncated: bool,
}

/// Reads the answer, at most `MAX_RESPONSE_BYTES` and `margin` more bytes.
pub async fn read(mut response: Response, margin: usize) -> Result<Answer, reqwest_mcp::Error> {
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase());
    let limit = MAX_RESPONSE_BYTES + margin;
    let mut body = Vec::new();
    let mut more = false;
    while let Some(chunk) = response.chunk().await? {
        let room = limit - body.len();
        if chunk.len() > room {
            body.extend_from_slice(&chunk[..room]);
            more = true;
            break;
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Answer {
        status,
        content_type,
        truncated: more || body.len() > MAX_RESPONSE_BYTES,
        body,
    })
}

impl Answer {
    /// What the model reads: `{ "statut": 200, "corps": … }`, and
    /// `"tronque"` when the body was cut.
    pub fn content(&self, redactor: &Redactor) -> Value {
        let mut content = json!({ "statut": self.status, "corps": self.body(redactor) });
        if self.truncated {
            content["tronque"] = json!(format!(
                "corps coupé après {} Kio",
                MAX_RESPONSE_BYTES / 1024
            ));
        }
        content
    }

    fn body(&self, redactor: &Redactor) -> Value {
        if self.body.is_empty() {
            return Value::Null;
        }
        let media = self
            .content_type
            .as_deref()
            .map(|value| value.split(';').next().unwrap_or_default().trim());
        let json = media.is_some_and(|m| m == "application/json" || m.ends_with("+json"));
        if json
            && !self.truncated
            && let Ok(value) = serde_json::from_slice::<Value>(&self.body)
        {
            return redactor.value(value);
        }
        let text = match media {
            None => match std::str::from_utf8(&self.body) {
                Ok(text) => Some(text.to_string()),
                // Text cut by the bound in the middle of a character.
                Err(error) if error.error_len().is_none() => {
                    Some(String::from_utf8_lossy(&self.body[..error.valid_up_to()]).into_owned())
                }
                Err(_) => None,
            },
            Some(media) if textual(media) => Some(String::from_utf8_lossy(&self.body).into_owned()),
            Some(_) => None,
        };
        match text {
            Some(text) => Value::String(self.cut(redactor.text(&text), redactor)),
            None => json!(format!(
                "[contenu {} de {}, non lu]",
                media.unwrap_or("binaire"),
                self.size()
            )),
        }
    }

    /// How long the body is, as far as it was read.
    fn size(&self) -> String {
        if self.truncated {
            format!("plus de {} Kio", MAX_RESPONSE_BYTES / 1024)
        } else {
            format!("{} octets", self.body.len())
        }
    }

    /// The masked text, cut at the bound when the body was: the token's
    /// longest spelling is dropped from its end first, which a cut through
    /// the token may have left unmasked.
    fn cut(&self, masked: String, redactor: &Redactor) -> String {
        if !self.truncated {
            return masked;
        }
        let kept = masked
            .chars()
            .count()
            .saturating_sub(redactor.longest())
            .min(MAX_RESPONSE_BYTES);
        masked.chars().take(kept).collect()
    }
}

/// Whether a media type is text to read.
fn textual(media: &str) -> bool {
    media.starts_with("text/")
        || [
            "json",
            "xml",
            "javascript",
            "x-www-form-urlencoded",
            "yaml",
            "csv",
        ]
        .iter()
        .any(|kind| media.ends_with(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(content_type: Option<&str>, body: Vec<u8>, truncated: bool) -> Answer {
        Answer {
            status: 200,
            content_type: content_type.map(str::to_string),
            body,
            truncated,
        }
    }

    #[test]
    fn says_what_it_did_not_read_and_how_much_there_was() {
        let none = Redactor::default();
        let image = answer(Some("image/png"), vec![0x89, b'P'], false);
        assert_eq!(image.body(&none), "[contenu image/png de 2 octets, non lu]");
        let large = answer(None, vec![0xff; MAX_RESPONSE_BYTES + 1], true);
        assert_eq!(
            large.body(&none),
            "[contenu binaire de plus de 256 Kio, non lu]"
        );
    }

    #[test]
    fn reads_as_text_an_untyped_text_cut_in_a_character() {
        let mut body = "é".repeat(10).into_bytes();
        body.push(0xc3);
        let cut = answer(None, body, true);
        assert_eq!(cut.body(&Redactor::default()), "é".repeat(10));
        let typed = answer(
            Some("application/json; charset=utf-8"),
            b"{\"a\": 1}".to_vec(),
            false,
        );
        assert_eq!(typed.body(&Redactor::default()), json!({ "a": 1 }));
    }
}
