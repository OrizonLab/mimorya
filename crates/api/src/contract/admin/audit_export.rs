//! The tokens with which the company's SIEM reads its audit log
//! (`audit:export`, company-wide, ADR-181), and where it reads it; each
//! token as any other a system holds of the company (`bearer_tokens.rs`).

use mimorya_db::repositories::bearer_tokens::BearerTokenRow;
use serde::Serialize;
use ts_rs::TS;

use super::bearer_tokens::{BearerTokenView, NAME_TAKEN};
use crate::services::bearer_tokens::MAX_TOKENS;

/// `GET /admin/audit/export-tokens`: the company's tokens, whether this
/// server seals the audit log, without which nothing is exported, the
/// address the SIEM reads, and the limits a new token meets.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportTokens {
    pub tokens: Vec<BearerTokenView>,
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
    pub fn of(tokens: &[BearerTokenRow], sealing: bool, web_origin: &str) -> Self {
        Self {
            tokens: tokens.iter().map(BearerTokenView::from).collect(),
            sealing,
            export_url: export_url(web_origin),
            max_tokens: MAX_TOKENS,
            name_length: NAME_TAKEN as u32,
        }
    }
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
