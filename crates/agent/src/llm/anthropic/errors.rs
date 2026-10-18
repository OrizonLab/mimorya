//! Maps Messages API failures to the provider-neutral error taxonomy.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::llm::types::{LlmError, LlmErrorKind};

const PROVIDER: &str = "anthropic";

static OVERFLOW_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)prompt is too long|context window").expect("valid regex"));

fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| body.chars().take(300).collect())
}

/// An HTTP error response.
pub fn from_status(status: u16, body: &str) -> LlmError {
    let error = |kind, message: String| LlmError::new(PROVIDER, kind, message);
    match status {
        401 | 403 => error(
            LlmErrorKind::Auth,
            "Authentification refusée par Anthropic".into(),
        ),
        429 => error(
            LlmErrorKind::RateLimit,
            "Limite de débit Anthropic atteinte".into(),
        ),
        400 => {
            let message = api_message(body);
            let kind = if OVERFLOW_RE.is_match(&message) {
                LlmErrorKind::ContextOverflow
            } else {
                LlmErrorKind::InvalidRequest
            };
            error(kind, message)
        }
        500..=599 => error(
            LlmErrorKind::Overloaded,
            "Service Anthropic indisponible".into(),
        ),
        _ => error(LlmErrorKind::Unknown, api_message(body)),
    }
}

/// An `error` event received in the middle of a stream.
pub fn from_stream_error(data: &Value) -> LlmError {
    let message = data["error"]["message"]
        .as_str()
        .unwrap_or("Erreur Anthropic")
        .to_string();
    let kind = match data["error"]["type"].as_str() {
        Some("overloaded_error" | "api_error") => LlmErrorKind::Overloaded,
        Some("rate_limit_error") => LlmErrorKind::RateLimit,
        Some("authentication_error" | "permission_error") => LlmErrorKind::Auth,
        Some("invalid_request_error") => LlmErrorKind::InvalidRequest,
        _ => LlmErrorKind::Unknown,
    };
    LlmError::new(PROVIDER, kind, message)
}

pub fn network() -> LlmError {
    LlmError::new(
        PROVIDER,
        LlmErrorKind::Network,
        "Connexion à Anthropic impossible",
    )
}

/// The stream could not be parsed (e.g. malformed tool input JSON).
pub fn malformed() -> LlmError {
    LlmError::new(
        PROVIDER,
        LlmErrorKind::MalformedOutput,
        "Réponse du modèle illisible",
    )
}
