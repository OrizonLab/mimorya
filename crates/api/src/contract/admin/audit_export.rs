//! The tokens with which the company's SIEM reads its audit log
//! (`audit:export`, company-wide, ADR-181): listed without their secret,
//! made with a name and shown once, removed.

use mimorya_db::repositories::audit_export_tokens::AuditExportTokenRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::{iso, trimmed};

/// A token as listed: never its secret.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportTokenView {
    pub id: Uuid,
    pub name: String,
    pub created_by: Option<Uuid>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl From<&AuditExportTokenRow> for AuditExportTokenView {
    fn from(row: &AuditExportTokenRow) -> Self {
        Self {
            id: row.id,
            name: row.name.clone(),
            created_by: row.created_by,
            created_at: iso(&row.created_at),
            last_used_at: row.last_used_at.as_ref().map(iso),
        }
    }
}

/// `GET /admin/audit/export-tokens`: the company's tokens, and whether this
/// server seals the audit log, without which nothing is exported.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportTokens {
    pub tokens: Vec<AuditExportTokenView>,
    pub sealing: bool,
}

/// Body of `POST /admin/audit/export-tokens`.
#[derive(Debug, Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewAuditExportToken {
    /// What the token is for: the SIEM that holds it.
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 80))]
    pub name: String,
}

/// `POST /admin/audit/export-tokens`: the token made, and its secret, shown
/// this once.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportTokenCreated {
    pub token: AuditExportTokenView,
    pub secret: String,
}
