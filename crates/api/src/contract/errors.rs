//! What every route answers when it refuses or fails: a code the client can
//! act on and a French message it can show. An internal error reveals
//! nothing about the internals.

use mimorya_core::ErrorCode;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    Unauthenticated,
    Forbidden,
    /// Absent, or out of the caller's scope (indistinguishable on purpose).
    NotFound,
    Conflict,
    BudgetExceeded,
    Unavailable,
    /// A malformed request (content type, body size…).
    Request,
    RateLimited,
    Internal,
}

impl From<ErrorCode> for ErrorKind {
    fn from(code: ErrorCode) -> Self {
        match code {
            ErrorCode::Validation => Self::Validation,
            ErrorCode::Unauthenticated => Self::Unauthenticated,
            ErrorCode::Forbidden => Self::Forbidden,
            ErrorCode::NotFound => Self::NotFound,
            ErrorCode::Conflict => Self::Conflict,
            ErrorCode::BudgetExceeded => Self::BudgetExceeded,
            ErrorCode::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Debug, Serialize, TS)]
pub struct ErrorDetail {
    pub code: ErrorKind,
    pub message: String,
    /// What the client may need beyond the message: the invalid fields
    /// (`{ issues: [{ path, message }] }`), or why access is held back
    /// (`{ reason: "password_change_required" }`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub details: Option<Value>,
}

/// The body of every error response.
#[derive(Debug, Serialize, TS)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}
