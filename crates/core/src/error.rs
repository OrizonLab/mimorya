//! Domain errors. They carry a stable `code` so transport layers (HTTP,
//! gateways) can map them without string matching, and a message safe to show
//! to end users.

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

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

/// A new code stops compiling here: list it in `ErrorCode::ALL` too, which
/// the API contract's tests go through.
#[cfg(test)]
fn _listed(code: ErrorCode) {
    match code {
        ErrorCode::Forbidden
        | ErrorCode::NotFound
        | ErrorCode::Validation
        | ErrorCode::Conflict
        | ErrorCode::Unauthenticated
        | ErrorCode::BudgetExceeded
        | ErrorCode::Unavailable => {}
    }
}

/// An identifier a request sent in its body that designates nothing the
/// company has (absent, or another company's): never sent back, it lets the
/// administration trace the attempt (ADR-054).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownReference {
    /// What the identifier was meant to designate (`role`, `org_unit`, `user`).
    pub resource_type: &'static str,
    pub id: Uuid,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct DomainError {
    pub code: ErrorCode,
    /// Message shown to users (French).
    pub message: String,
    pub details: Option<Value>,
    /// Set by `unknown`; not part of the answer. Boxed: errors stay small.
    pub reference: Option<Box<UnknownReference>>,
}

impl DomainError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            reference: None,
        }
    }

    /// An identifier sent in the body that designates nothing the company
    /// has: answered as an invalid request, like any other, the identifier
    /// kept aside for the audit.
    pub fn unknown(message: impl Into<String>, resource_type: &'static str, id: Uuid) -> Self {
        Self {
            reference: Some(Box::new(UnknownReference { resource_type, id })),
            ..Self::validation(message)
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
