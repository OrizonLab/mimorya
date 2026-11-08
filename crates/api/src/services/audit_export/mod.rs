//! The company's audit log read by its SIEM (ADR-181): tokens made, listed
//! and removed by who may export it (`tokens.rs`), and the sealed entries a
//! token reads, in the chain's order, page after page (`reading.rs`).

mod reading;
mod tokens;

pub use reading::{DEFAULT_PAGE, ExportPage, MAX_PAGE, Resume, export_audit_log};
pub use tokens::{
    TOKEN_PREFIX, audit_export_tokens, create_audit_export_token, delete_audit_export_token,
};
