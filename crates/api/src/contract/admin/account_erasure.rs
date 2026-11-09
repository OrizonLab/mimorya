//! An account erased by an administrator (ADR-189): its address typed
//! again, and what went with it, by its counts.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::trimmed;

/// Body of `POST /admin/users/{id}/erasure`: the account's address, typed
/// again by the administrator.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountErasure {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 320))]
    pub confirm_email: String,
}

/// `POST /admin/users/{id}/erasure`: what the account took with it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ErasedAccount {
    pub conversations: i64,
    pub messages: i64,
    pub memories: i64,
    pub runs: i64,
}
