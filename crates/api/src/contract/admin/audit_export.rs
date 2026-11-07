//! The tokens with which the company's SIEM reads its audit log
//! (`audit:export`, company-wide, ADR-181): listed without their secret,
//! made with a name and shown once, removed.

use mimorya_db::repositories::audit_export_tokens::AuditExportTokenRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::{iso, trimmed};
use crate::services::audit_export::{MAX_TOKENS, NAME_LENGTH};

/// The longest name, as the validation counts it.
const NAME_TAKEN: u64 = NAME_LENGTH as u64;

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

/// `GET /admin/audit/export-tokens`: the company's tokens, whether this
/// server seals the audit log, without which nothing is exported, the
/// address the SIEM reads, and the limits a new token meets.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportTokens {
    pub tokens: Vec<AuditExportTokenView>,
    pub sealing: bool,
    pub export_url: String,
    /// The tokens a company may keep.
    pub max_tokens: i64,
    /// The longest name a token takes, in characters.
    pub name_length: u32,
}

/// The address a SIEM reads the audit log at, from the application's.
pub fn export_url(web_origin: &str) -> String {
    format!("{}/api/audit/export", web_origin.trim_end_matches('/'))
}

impl AuditExportTokens {
    /// The company's `tokens`, as a server that seals or not (`sealing`)
    /// and the application at `web_origin` show them.
    pub fn of(tokens: &[AuditExportTokenRow], sealing: bool, web_origin: &str) -> Self {
        Self {
            tokens: tokens.iter().map(AuditExportTokenView::from).collect(),
            sealing,
            export_url: export_url(web_origin),
            max_tokens: MAX_TOKENS,
            name_length: NAME_TAKEN as u32,
        }
    }
}

/// Body of `POST /admin/audit/export-tokens`.
#[derive(Debug, Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewAuditExportToken {
    /// What the token is for: the SIEM that holds it.
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = NAME_TAKEN))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_address_is_the_application_s() {
        assert_eq!(
            export_url("https://mimorya.example/"),
            "https://mimorya.example/api/audit/export"
        );
    }
}
