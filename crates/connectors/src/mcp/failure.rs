//! What an administrator, or the model, reads when an MCP server cannot be
//! used: what happened, in French, then the most precise cause the transport
//! gave, with the token masked (`outbound::failure`). The type names of the
//! MCP library never reach that message.

use std::error::Error as StdError;

use rmcp::ServiceError;
use rmcp::service::ClientInitializeError;
use rmcp::transport::DynamicTransportError;
use rmcp::transport::streamable_http_client::StreamableHttpError;
use rmcp::transport::worker::WorkerQuitReason;

use super::http::{HttpError, TOO_LONG};
use crate::outbound::{
    ACCESS_DENIED, Cause, Redactor, TOO_SLOW, explained, request_cause as request,
};

type WorkerError = WorkerQuitReason<HttpError>;

pub const NOT_MCP: &str = "Le serveur n’a pas répondu comme un serveur MCP";
const CLOSED: &str = "La connexion au serveur MCP est fermée";
const REFUSED_CALL: &str = "Le serveur MCP a renvoyé une erreur";

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
        // Said by the platform: nothing of the server's answer to add.
        ServiceError::McpError(data) if data.message == TOO_LONG => Cause {
            summary: TOO_LONG,
            detail: String::new(),
        },
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
