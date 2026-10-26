//! What an administrator, or the model, reads when a company's server cannot
//! be reached: what happened, in French, then the most precise cause the
//! HTTP client gave, abridged, with the token masked. Shared by MCP servers
//! (ADR-025) and REST APIs (ADR-112).

use std::error::Error as StdError;

use super::redact::Redactor;

pub const NOT_PUBLIC: &str = "Adresse refusée : elle ne mène pas à un serveur public";
pub const UNREACHABLE: &str = "Serveur injoignable à cette adresse";
pub const ACCESS_DENIED: &str = "Accès refusé par le serveur : jeton manquant ou invalide";
pub const TOO_SLOW: &str = "Le serveur n’a pas répondu à temps";
const MAX_DETAIL_CHARS: usize = 200;

/// A failure's summary and the cause the transport gave.
pub struct Cause {
    pub summary: &'static str,
    pub detail: String,
}

/// The failure as it is told: summary, then the masked, abridged cause.
pub fn explained(cause: Cause, redactor: &Redactor) -> anyhow::Error {
    let detail: String = redactor
        .text(&cause.detail)
        .chars()
        .take(MAX_DETAIL_CHARS)
        .collect();
    if detail.trim().is_empty() {
        anyhow::anyhow!("{}", cause.summary)
    } else {
        anyhow::anyhow!("{} (détail : {detail})", cause.summary)
    }
}

/// An HTTP request that got no answer: refused address, timeout or
/// unreachable server.
pub fn request_cause(error: &reqwest_mcp::Error) -> Cause {
    let mut causes = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        causes.push(cause.to_string());
        source = cause.source();
    }
    // The innermost cause, without the address already shown to the administrator.
    let detail = causes.last().cloned().unwrap_or_default();
    let summary = if causes.iter().any(|c| c.contains("aucune adresse publique")) {
        NOT_PUBLIC
    } else if error.is_timeout() {
        TOO_SLOW
    } else {
        UNREACHABLE
    };
    Cause { summary, detail }
}
