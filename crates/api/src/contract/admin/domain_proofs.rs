//! The proof of the company's e-mail domains (`settings:manage`, ADR-069):
//! each domain its providers cover, the DNS record that proves it, and
//! whether it is proved.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::trimmed;

/// A domain the company's providers cover.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DomainProofView {
    pub domain: String,
    /// The TXT record to publish in the domain's DNS: where, and its value.
    pub record_name: String,
    pub record_value: String,
    /// When the company last proved it; absent: never.
    pub verified_at: Option<String>,
}

/// Body of `POST /admin/identity-providers/domain-proofs`: the domain whose
/// record the company published, to read now.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainProofRequest {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 253))]
    pub domain: String,
}
