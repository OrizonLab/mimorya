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
            None => std::str::from_utf8(&self.body).ok().map(str::to_string),
            Some(media) if textual(media) => Some(String::from_utf8_lossy(&self.body).into_owned()),
            Some(_) => None,
        };
        match text {
            Some(text) => Value::String(self.cut(redactor.text(&text), redactor)),
            None => json!(format!(
                "[contenu {} de {} octets, non lu]",
                media.unwrap_or("binaire"),
                self.body.len()
            )),
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
