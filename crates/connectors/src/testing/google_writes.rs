//! What the test Gmail and Calendar write (ADR-123): an e-mail sent (its
//! identifier naming its owner) and an event created (its times given back
//! in the platform's zone, as Google gives them in the calendar's), each
//! recorded with what it received; or refused with the status a test
//! chose. `SentMail` reads back the message Gmail received.

use axum::Json;
use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::alphabet::URL_SAFE;
use base64::engine::general_purpose::STANDARD;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use chrono::DateTime;
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use serde_json::{Value, json};

use super::google::{Shared, failure, owner};

/// Gmail's `raw`, padded or not.
const RAW: GeneralPurpose = GeneralPurpose::new(
    &URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// The owner of the token, once the write is recorded and not refused.
fn writer(
    shared: &Shared,
    headers: &HeaderMap,
    path: &str,
    raw: Option<String>,
    body: Value,
) -> Result<String, StatusCode> {
    let (owner, _) = owner(shared, headers, path.into(), raw, Some(body))?;
    match shared.lock().expect("google").refused_writes {
        Some(status) => Err(status),
        None => Ok(owner),
    }
}

pub(super) async fn send_message(
    State(shared): State<Shared>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    Json(body): Json<Value>,
) -> Response {
    match writer(&shared, &headers, "/users/me/messages/send", raw, body) {
        Ok(owner) => {
            let id = format!("{owner}-envoye");
            Json(json!({ "id": id, "threadId": id, "labelIds": ["SENT"] })).into_response()
        }
        Err(refused) => failure(refused),
    }
}

/// An instant sent, as Google gives it back in the calendar's zone.
fn in_platform_zone(time: &Value) -> Value {
    let sent = time["dateTime"]
        .as_str()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok());
    match sent {
        Some(at) => json!({
            "dateTime": at.with_timezone(&PLATFORM_TIME_ZONE).to_rfc3339(),
            "timeZone": time["timeZone"],
        }),
        None => time.clone(),
    }
}

pub(super) async fn create_event(
    State(shared): State<Shared>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    Json(body): Json<Value>,
) -> Response {
    let path = "/calendars/primary/events";
    let owner = match writer(&shared, &headers, path, raw, body.clone()) {
        Ok(owner) => owner,
        Err(refused) => return failure(refused),
    };
    Json(json!({
        "id": format!("{owner}-nouveau"),
        "status": "confirmed",
        "summary": body["summary"],
        "start": in_platform_zone(&body["start"]),
        "end": in_platform_zone(&body["end"]),
        "htmlLink": format!("https://calendar.example/{owner}/nouveau"),
    }))
    .into_response()
}

/// A message Gmail received (`raw`), read back: its headers, unfolded and
/// their encoded words decoded, and its text, its lines ended as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMail {
    pub headers: Vec<(String, String)>,
    pub text: String,
}

impl SentMail {
    pub fn read(raw: &str) -> Option<Self> {
        let message = String::from_utf8(RAW.decode(raw).ok()?).ok()?;
        let (head, body) = message.split_once("\r\n\r\n")?;
        let headers = head
            .replace("\r\n ", " ")
            .split("\r\n")
            .map(|line| {
                let (name, value) = line.split_once(": ")?;
                Some((name.to_string(), decoded(value)?))
            })
            .collect::<Option<Vec<_>>>()?;
        let text = STANDARD.decode(body.replace("\r\n", "")).ok()?;
        Some(Self {
            headers,
            text: String::from_utf8(text).ok()?,
        })
    }

    /// The value of a header, its name in any case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A header's value, its encoded words decoded and joined, the space
/// between two dropped (RFC 2047, 6.2).
fn decoded(value: &str) -> Option<String> {
    if !value.starts_with("=?") {
        return Some(value.to_string());
    }
    let mut bytes = Vec::new();
    for word in value.split(' ') {
        let inner = word.strip_prefix("=?UTF-8?B?")?.strip_suffix("?=")?;
        bytes.extend(STANDARD.decode(inner).ok()?);
    }
    String::from_utf8(bytes).ok()
}
