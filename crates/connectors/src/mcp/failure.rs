//! What an administrator, or the model, reads when an MCP server cannot be
//! used: what happened, in French, then the most precise cause the transport
//! gave, with the token masked (`outbound::failure`). The type names of the
//! MCP library never reach that message. A server whose refusal points to
//! its OAuth metadata says it wants an OAuth authorization; one that
//! refuses the token of its authorization says to authorize it again
//! (ADR-171).

use std::error::Error as StdError;

use rmcp::ServiceError;
use rmcp::service::ClientInitializeError;
use rmcp::transport::DynamicTransportError;
use rmcp::transport::streamable_http_client::StreamableHttpError;
use rmcp::transport::worker::WorkerQuitReason;

use super::http::{HttpError, TOO_LONG};
use super::oauth::bearer_challenge;
use crate::outbound::{
    ACCESS_DENIED, Cause, Redactor, TOO_SLOW, explained, request_cause as request,
};

type WorkerError = WorkerQuitReason<HttpError>;

pub const NOT_MCP: &str = "Le serveur n’a pas répondu comme un serveur MCP";
/// A refusal, without a token, whose challenge names where the server's
/// authorization is described (RFC 9728, ADR-171).
pub const OAUTH_REQUIRED: &str =
    "Serveur protégé par OAuth : il refuse les appels sans une autorisation OAuth";
/// The same refusal of a token: the server may take tokens of its own as
/// well as OAuth ones, and only says where its OAuth metadata is.
pub const OAUTH_OFFERED: &str = "Accès refusé par le serveur : jeton invalide ; ce serveur annonce aussi une autorisation \
     OAuth";
/// A refusal of the access token of the server's own authorization:
/// revoked, expired early, or without the scopes the call needs.
pub const OAUTH_REFUSED: &str = "Le serveur refuse le jeton de son autorisation OAuth (révoqué, \
     expiré ou sans les portées voulues) : un administrateur doit autoriser de nouveau ce \
     connecteur";
const CLOSED: &str = "La connexion au serveur MCP est fermée";

/// Whose token the platform sends the server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TokenKind {
    /// Typed by an administrator, if any.
    #[default]
    Typed,
    /// The access token of the server's OAuth authorization (ADR-171).
    Authorization,
}
const REFUSED_CALL: &str = "Le serveur MCP a renvoyé une erreur";

/// A failed start, explained.
pub fn start_failed(error: &anyhow::Error, redactor: &Redactor, kind: TokenKind) -> anyhow::Error {
    let cause = error.chain().find_map(known).unwrap_or_else(|| Cause {
        summary: NOT_MCP,
        detail: error.root_cause().to_string(),
    });
    explained(with_token(cause, redactor, kind), redactor)
}

/// A refusal of the token sent, told by whose it is: one typed by hand
/// refused by a server that announces OAuth, or the authorization's.
fn with_token(cause: Cause, redactor: &Redactor, kind: TokenKind) -> Cause {
    let refused = cause.summary == OAUTH_REQUIRED || cause.summary == ACCESS_DENIED;
    let summary = match kind {
        TokenKind::Authorization if refused => OAUTH_REFUSED,
        TokenKind::Typed if cause.summary == OAUTH_REQUIRED && redactor.longest() > 0 => {
            OAUTH_OFFERED
        }
        _ => return cause,
    };
    Cause { summary, ..cause }
}

/// A failed tool call, explained.
pub fn call_failed(error: &ServiceError, redactor: &Redactor, kind: TokenKind) -> anyhow::Error {
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
    explained(with_token(cause, redactor, kind), redactor)
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

/// Whether the server refused the message with `status`, without an error
/// of its own (ADR-164: a 429 is an answer).
pub fn refused_with(error: &ServiceError, status: u16) -> bool {
    let ServiceError::TransportSend(error) = error else {
        return false;
    };
    let said = format!("HTTP {status} ");
    matches!(
        http_error(error.error.as_ref()),
        Some(StreamableHttpError::UnexpectedServerResponse(text)) if text.starts_with(&said)
    )
}

/// The HTTP transport's error, wherever it sits.
fn http_error<'a>(error: &'a (dyn StdError + 'static)) -> Option<&'a HttpError> {
    if let Some(error) = error.downcast_ref::<HttpError>() {
        return Some(error);
    }
    if let Some(WorkerQuitReason::Fatal { error, .. }) = error.downcast_ref::<WorkerError>() {
        return Some(error);
    }
    let inner = error.downcast_ref::<DynamicTransportError>()?;
    http_error(inner.error.as_ref())
}

fn http(error: &HttpError) -> Cause {
    let detail = error.to_string();
    let summary = match error {
        StreamableHttpError::Client(error) => return request(error),
        StreamableHttpError::AuthRequired(refusal)
            if bearer_challenge(&refusal.www_authenticate_header)
                .is_some_and(|challenge| challenge.resource_metadata.is_some()) =>
        {
            OAUTH_REQUIRED
        }
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
