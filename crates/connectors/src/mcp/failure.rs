//! What an administrator, or the model, reads when an MCP server cannot be
//! used: what happened, in French, then the most precise cause the transport
//! gave, with the token masked. The type names of the MCP library never reach
//! that message.

use std::error::Error as StdError;

use rmcp::ServiceError;
use rmcp::service::ClientInitializeError;
use rmcp::transport::DynamicTransportError;
use rmcp::transport::streamable_http_client::StreamableHttpError;
use rmcp::transport::worker::WorkerQuitReason;

use super::redact::Redactor;

type HttpError = StreamableHttpError<reqwest_mcp::Error>;
type WorkerError = WorkerQuitReason<HttpError>;

pub const NOT_PUBLIC: &str = "Adresse refusée : elle ne mène pas à un serveur public";
pub const UNREACHABLE: &str = "Serveur injoignable à cette adresse";
pub const ACCESS_DENIED: &str = "Accès refusé par le serveur : jeton manquant ou invalide";
pub const TOO_SLOW: &str = "Le serveur n’a pas répondu à temps";
pub const NOT_MCP: &str = "Le serveur n’a pas répondu comme un serveur MCP";
const CLOSED: &str = "La connexion au serveur MCP est fermée";
const REFUSED_CALL: &str = "Le serveur MCP a renvoyé une erreur";
const MAX_DETAIL_CHARS: usize = 200;

struct Cause {
    summary: &'static str,
    detail: String,
}

/// A failed start, explained.
pub fn start_failed(error: &anyhow::Error, redactor: &Redactor) -> anyhow::Error {
    let cause = error.chain().find_map(known).unwrap_or_else(|| Cause {
        summary: NOT_MCP,
        detail: error.root_cause().to_string(),
    });
    explained(cause, redactor)
}

/// A failed tool call, explained.
pub fn call_failed(error: &ServiceError, redactor: &Redactor) -> anyhow::Error {
    let cause = match error {
        ServiceError::McpError(data) => Cause {
            summary: REFUSED_CALL,
            detail: data.message.to_string(),
        },
        ServiceError::Timeout { .. } => Cause {
            summary: TOO_SLOW,
            detail: error.to_string(),
        },
        ServiceError::TransportClosed => Cause {
            summary: CLOSED,
            detail: String::new(),
        },
        _ => known(error).unwrap_or_else(|| Cause {
            summary: REFUSED_CALL,
            detail: error.to_string(),
        }),
    };
    explained(cause, redactor)
}

fn explained(cause: Cause, redactor: &Redactor) -> anyhow::Error {
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

/// The transport errors this module knows how to explain, wherever they sit
/// in the chain (the MCP library does not always link them as sources).
fn known(error: &(dyn StdError + 'static)) -> Option<Cause> {
    if let Some(ClientInitializeError::TransportError { error, .. }) = error.downcast_ref() {
        return transport(error);
    }
    if let Some(ServiceError::TransportSend(error)) = error.downcast_ref() {
        return transport(error);
    }
    if let Some(error) = error.downcast_ref::<DynamicTransportError>() {
        return transport(error);
    }
    if let Some(WorkerQuitReason::Fatal { error, .. }) = error.downcast_ref::<WorkerError>() {
        return Some(http(error));
    }
    if let Some(error) = error.downcast_ref::<HttpError>() {
        return Some(http(error));
    }
    error.downcast_ref::<reqwest_mcp::Error>().map(request)
}

fn transport(error: &DynamicTransportError) -> Option<Cause> {
    known(error.error.as_ref())
}

fn http(error: &HttpError) -> Cause {
    let detail = error.to_string();
    let summary = match error {
        StreamableHttpError::Client(error) => return request(error),
        StreamableHttpError::AuthRequired(_) | StreamableHttpError::InsufficientScope(_) => {
            ACCESS_DENIED
        }
        StreamableHttpError::UnexpectedServerResponse(text)
            if text.starts_with("HTTP 401") || text.starts_with("HTTP 403") =>
        {
            ACCESS_DENIED
        }
        _ => NOT_MCP,
    };
    Cause { summary, detail }
}

fn request(error: &reqwest_mcp::Error) -> Cause {
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
