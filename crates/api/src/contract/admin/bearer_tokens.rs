//! The tokens another system holds of the company (ADR-181, ADR-183), as
//! the administration sees them: listed without their secret, made with a
//! name and shown once, removed.

use mimorya_db::repositories::bearer_tokens::BearerTokenRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::{iso, trimmed};
use crate::services::bearer_tokens::NAME_LENGTH;

/// The longest name, as the validation counts it.
pub(super) const NAME_TAKEN: u64 = NAME_LENGTH as u64;

/// A token as listed: never its secret.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BearerTokenView {
    pub id: Uuid,
    pub name: String,
    pub created_by: Option<Uuid>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl From<&BearerTokenRow> for BearerTokenView {
    fn from(row: &BearerTokenRow) -> Self {
        Self {
            id: row.id,
            name: row.name.clone(),
            created_by: row.created_by,
            created_at: iso(&row.created_at),
            last_used_at: row.last_used_at.as_ref().map(iso),
        }
    }
}

/// Body of the making of a token (`POST /admin/audit/export-tokens`,
/// `POST /admin/scim/tokens`).
#[derive(Debug, Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewBearerToken {
    /// What the token is for: the system that holds it.
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = NAME_TAKEN))]
    pub name: String,
}

/// A token made, and its secret, shown this once.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BearerTokenCreated {
    pub token: BearerTokenView,
    pub secret: String,
}

impl BearerTokenCreated {
    pub fn of(token: &BearerTokenRow, secret: String) -> Self {
        Self {
            token: BearerTokenView::from(token),
            secret,
        }
    }
}
