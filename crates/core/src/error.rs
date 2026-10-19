//! Domain errors. They carry a stable `code` so transport layers (HTTP,
//! gateways) can map them without string matching, and a message safe to show
//! to end users.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Forbidden,
    NotFound,
    Validation,
    Conflict,
    Unauthenticated,
    BudgetExceeded,
    Unavailable,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 7] = [
        Self::Forbidden,
        Self::NotFound,
        Self::Validation,
        Self::Conflict,
        Self::Unauthenticated,
        Self::BudgetExceeded,
        Self::Unavailable,
    ];
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct DomainError {
    pub code: ErrorCode,
    /// Message shown to users (French).
    pub message: String,
    pub details: Option<Value>,
}

impl DomainError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }

    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthenticated, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, message)
    }
}
