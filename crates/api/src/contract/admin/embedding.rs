//! The company's embedding model (`settings:manage`, ADR-074): what it may
//! choose, its choice, how far its memory is indexed, and the connection
//! test. The API key is write-only: never returned, only whether one is
//! stored.

use mimorya_db::repositories::embedding_settings::EmbeddingSettingsRow;
use mimorya_db::repositories::memory_embeddings::IndexProgress;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::{iso, trimmed, trimmed_opt};
use crate::services::admin::EmbeddingTestOutcome;

/// A kind of embedding provider a company may choose.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingKind {
    pub kind: String,
    pub requires_key: bool,
    /// Served at an endpoint the operator lists.
    pub listed_endpoint: bool,
    pub default_model: Option<String>,
    /// Where to start the closeness a memory needs, for this kind.
    pub default_min_similarity: f32,
}

/// The company's embedding model.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingSettingsView {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub min_similarity: f32,
    pub key_stored: bool,
    /// The stored configuration can serve (key readable, endpoint still listed…).
    pub usable: bool,
    /// Why it cannot, when it cannot (French).
    pub problem: Option<String>,
    /// Active memories with a vector of this model.
    pub embedded: i64,
    /// Active memories.
    pub memories: i64,
    /// Why the last indexing failed, until one succeeds (French).
    pub indexing_error: Option<String>,
    pub updated_at: String,
}

impl EmbeddingSettingsView {
    pub fn new(
        s: &EmbeddingSettingsRow,
        key_stored: bool,
        problem: Option<String>,
        progress: IndexProgress,
        indexing_error: Option<String>,
    ) -> Self {
        Self {
            provider: s.provider.clone(),
            model: s.model.clone(),
            base_url: s.base_url.clone(),
            min_similarity: s.min_similarity,
            key_stored,
            usable: problem.is_none(),
            problem,
            embedded: progress.embedded,
            memories: progress.total,
            indexing_error,
            updated_at: iso(&s.updated_at),
        }
    }
}

/// `GET /admin/embedding`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingView {
    /// The server can seal keys (`MIMORYA_MASTER_KEY`).
    pub secrets_available: bool,
    /// `None`: the company's memory is searched by its words only.
    pub settings: Option<EmbeddingSettingsView>,
    pub providers: Vec<EmbeddingKind>,
    /// Endpoints the operator lists for self-hosted servers.
    pub endpoints: Vec<String>,
}

/// `POST /admin/embedding/test`.
#[derive(Debug, Serialize, TS)]
pub struct EmbeddingTestResult {
    pub ok: bool,
    pub provider: String,
    pub model: String,
    /// The provider's refusal, as shown to users (never the key).
    pub error: Option<String>,
}

impl From<EmbeddingTestOutcome> for EmbeddingTestResult {
    fn from(outcome: EmbeddingTestOutcome) -> Self {
        Self {
            ok: outcome.error.is_none(),
            provider: outcome.provider,
            model: outcome.model,
            error: outcome.error,
        }
    }
}

/// Body of `PUT /admin/embedding`: the company's embedding model.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingChoice {
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
    /// Absent: the provider's starting value.
    #[serde(default)]
    #[ts(optional)]
    pub min_similarity: Option<f32>,
    /// Absent: the stored key is kept (same endpoint only).
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub api_key: Option<String>,
}
