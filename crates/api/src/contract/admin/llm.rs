//! The company's model providers (`settings:manage`, ADR-024): what its runs
//! use, what it may choose, its choice, the one it chose apart to review
//! their answers (ADR-108), and the connection test. An API key is
//! write-only: never returned, only whether one is stored.

use mimorya_db::repositories::llm_settings::LlmSettingsRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::{iso, trimmed, trimmed_opt};
use crate::services::admin::LlmTestOutcome;

/// A kind of provider a company may choose.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKind {
    pub kind: String,
    pub requires_key: bool,
    /// Served at an endpoint the operator lists.
    pub listed_endpoint: bool,
    pub default_model: Option<String>,
}

/// A provider of the company's own, for its runs or for reviews.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettingsView {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub key_stored: bool,
    /// Present in the price table: costs are estimated.
    pub priced: bool,
    /// The stored configuration can serve (key readable, endpoint still listed…).
    pub usable: bool,
    /// Why it cannot, when it cannot (French).
    pub problem: Option<String>,
    pub updated_at: String,
}

impl LlmSettingsView {
    pub fn new(
        s: &LlmSettingsRow,
        key_stored: bool,
        priced: bool,
        problem: Option<String>,
    ) -> Self {
        Self {
            provider: s.provider.clone(),
            model: s.model.clone(),
            base_url: s.base_url.clone(),
            key_stored,
            priced,
            usable: problem.is_none(),
            problem,
            updated_at: iso(&s.updated_at),
        }
    }
}

/// The provider the platform offers every company.
#[derive(Debug, Serialize, TS)]
pub struct PlatformModel {
    pub provider: String,
    pub model: String,
    pub priced: bool,
}

/// `GET /admin/llm`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LlmView {
    /// The server can seal keys (`MIMORYA_MASTER_KEY`).
    pub secrets_available: bool,
    pub platform: Option<PlatformModel>,
    pub settings: Option<LlmSettingsView>,
    /// The provider the company chose apart to review its answers (ADR-108);
    /// without it, or while it cannot be used, its runs' model reviews them.
    pub verification: Option<LlmSettingsView>,
    pub providers: Vec<ProviderKind>,
    /// Endpoints the operator lists for self-hosted providers.
    pub endpoints: Vec<String>,
}

/// `POST /admin/llm/test` and `POST /admin/llm/verification/test`.
#[derive(Debug, Serialize, TS)]
pub struct LlmTestResult {
    pub ok: bool,
    pub provider: String,
    pub model: String,
    /// The provider's refusal, as shown to users (never the key).
    pub error: Option<String>,
}

impl From<LlmTestOutcome> for LlmTestResult {
    fn from(outcome: LlmTestOutcome) -> Self {
        Self {
            ok: outcome.error.is_none(),
            provider: outcome.provider,
            model: outcome.model,
            error: outcome.error,
        }
    }
}

/// Body of `PUT /admin/llm` and `PUT /admin/llm/verification`: a provider of
/// the company's own.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmChoice {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 40))]
    pub provider: String,
    /// Absent or blank: the provider's default model.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub base_url: Option<String>,
    /// Absent: the stored key is kept (same endpoint only).
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub api_key: Option<String>,
}
