//! The tokens with which the company's identity provider provisions its
//! accounts through SCIM (`user:manage`, company-wide, ADR-183), and where
//! it reaches SCIM; each token as any other a system holds of the company
//! (`bearer_tokens.rs`).

use mimorya_db::repositories::bearer_tokens::BearerTokenRow;
use serde::Serialize;
use ts_rs::TS;

use super::bearer_tokens::{BearerTokenView, NAME_TAKEN};
use crate::contract::scim_url;
use crate::services::bearer_tokens::MAX_TOKENS;

/// `GET /admin/scim/tokens`: the company's tokens, where the provider
/// reaches SCIM, and the limits a new token meets.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScimTokens {
    pub tokens: Vec<BearerTokenView>,
    /// The SCIM base address to give the identity provider.
    pub scim_url: String,
    /// The tokens a company may keep.
    pub max_tokens: i64,
    /// The longest name a token takes, in characters.
    pub name_length: u32,
}

impl ScimTokens {
    /// The company's `tokens`, as the application at `web_origin` shows them.
    pub fn of(tokens: &[BearerTokenRow], web_origin: &str) -> Self {
        Self {
            tokens: tokens.iter().map(BearerTokenView::from).collect(),
            scim_url: scim_url(web_origin),
            max_tokens: MAX_TOKENS,
            name_length: NAME_TAKEN as u32,
        }
    }
}
