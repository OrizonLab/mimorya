//! Embedding models (ADR-074): what turns a memory, or a request, into a
//! vector, to search memory by meaning. A company chooses its own, or none:
//! then nothing of its memory leaves for one, and memory is searched by its
//! words only.

mod provider;
mod settings;

use async_trait::async_trait;

pub use provider::{OpenAiEmbedder, parse_embeddings};
pub use settings::{
    EMBEDDING_KINDS, EmbeddingConfig, MISTRAL_BASE_URL, TenantEmbeddingChoice,
    check_tenant_embedding, company_embedding_config, create_embedder, default_embedding_model,
    default_min_similarity, embedding_requires_key, embedding_uses_listed_endpoint,
};

use super::types::LlmError;

/// Dimensions of every vector stored (ADR-004): the column's.
pub const EMBEDDING_DIMENSIONS: usize = 1024;

/// Texts embedded in one request, at most.
pub const MAX_EMBEDDING_BATCH: usize = 32;

/// The vectors of the texts sent, in their order, and what they cost.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedded {
    pub vectors: Vec<Vec<f32>>,
    pub input_tokens: u64,
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// The kind of provider, as usage records name it.
    fn id(&self) -> &str;
    fn model(&self) -> &str;
    /// One vector of `EMBEDDING_DIMENSIONS` per text; at most
    /// `MAX_EMBEDDING_BATCH` texts.
    async fn embed(&self, texts: &[String]) -> Result<Embedded, LlmError>;
}
