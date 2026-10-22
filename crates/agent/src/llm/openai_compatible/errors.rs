//! Maps Chat Completions failures to the provider-neutral error taxonomy.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::llm::types::{LlmError, LlmErrorKind};

static OVERFLOW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)context length|too long|maximum context").expect("valid regex")
});

pub(crate) fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| body.chars().take(300).collect())
}

pub fn from_status(provider: &str, status: u16, body: &str) -> LlmError {
    let error = |kind, message: String| LlmError::new(provider, kind, message);
    match status {
        401 | 403 => error(
            LlmErrorKind::Auth,
            "Authentification refusée par le fournisseur".into(),
        ),
        429 => error(LlmErrorKind::RateLimit, "Limite de débit atteinte".into()),
        400 => {
            let message = api_message(body);
            let kind = if OVERFLOW_RE.is_match(&message) {
                LlmErrorKind::ContextOverflow
            } else {
                LlmErrorKind::InvalidRequest
            };
            error(kind, message)
        }
        500..=599 => error(LlmErrorKind::Overloaded, "Fournisseur indisponible".into()),
        _ => error(LlmErrorKind::Unknown, api_message(body)),
    }
}

pub fn network(provider: &str) -> LlmError {
    LlmError::new(
        provider,
        LlmErrorKind::Network,
        "Connexion au fournisseur impossible",
    )
}

pub fn malformed(provider: &str) -> LlmError {
    LlmError::new(
        provider,
        LlmErrorKind::MalformedOutput,
        "Réponse vide ou illisible du modèle",
    )
}
