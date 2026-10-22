//! The vector of a request (ADR-074), to search the company's memory by
//! its meaning: asked of the company's model with a short delay, since a
//! turn waits for it. Should the model fail, the words still find: the
//! failure is logged (never the text) and the search goes on without it.
//! What each call costs is recorded like the model's.

use std::time::Duration;

use mimorya_agent::llm::{Embedded, LlmUsage};
use mimorya_db::repositories::memory::Meaning;
use mimorya_db::repositories::usage::{NewUsage, append_usage};
use uuid::Uuid;

use super::{CompanyEmbedder, EmbeddingService};

/// How long a turn waits for the vector of its request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// What of a request is embedded: its beginning carries its sense.
const MAX_REQUEST_CHARS: usize = 2000;

/// A request's vector, with what it compares to.
pub struct RequestMeaning {
    pub vector: Vec<f32>,
    pub model: String,
    pub min_similarity: f32,
}

impl RequestMeaning {
    pub fn as_meaning(&self) -> Meaning<'_> {
        Meaning {
            vector: &self.vector,
            model: &self.model,
            min_similarity: self.min_similarity,
        }
    }
}

/// On whose behalf a vector is computed, for the usage records.
#[derive(Debug, Clone, Copy, Default)]
pub struct UsageBy {
    pub user_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
}

impl EmbeddingService {
    /// The meaning of `text` in the company: `None` when its memory is
    /// searched by words only, or its model fails now.
    pub async fn meaning_of(
        &self,
        tenant_id: Uuid,
        text: &str,
        by: UsageBy,
    ) -> Option<RequestMeaning> {
        let embedder = match self.embedder(tenant_id).await {
            Ok(Some(embedder)) => embedder,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%tenant_id, error = %error, "Embedding model resolution failed");
                return None;
            }
        };
        let text: String = text.chars().take(MAX_REQUEST_CHARS).collect();
        let asked = embedder.provider.embed(std::slice::from_ref(&text));
        let embedded = match tokio::time::timeout(REQUEST_TIMEOUT, asked).await {
            Ok(Ok(embedded)) => embedded,
            Ok(Err(error)) => {
                tracing::warn!(%tenant_id, error = %error, "Request not embedded");
                return None;
            }
            Err(_) => {
                tracing::warn!(%tenant_id, "Request not embedded in time");
                return None;
            }
        };
        if let Err(error) = self.record_usage(tenant_id, &embedder, &embedded, by).await {
            tracing::warn!(%tenant_id, error = %error, "Embedding usage not recorded");
        }
        let vector = embedded.vectors.into_iter().next()?;
        Some(RequestMeaning {
            vector,
            model: embedder.model,
            min_similarity: embedder.min_similarity,
        })
    }

    /// Records what a call cost, priced when the model is (never guessed).
    pub(crate) async fn record_usage(
        &self,
        tenant_id: Uuid,
        embedder: &CompanyEmbedder,
        embedded: &Embedded,
        by: UsageBy,
    ) -> anyhow::Result<()> {
        let usage = LlmUsage {
            input_tokens: embedded.input_tokens,
            ..LlmUsage::default()
        };
        let mut tx = self.db.begin(tenant_id).await?;
        append_usage(
            &mut tx,
            NewUsage {
                tenant_id,
                user_id: by.user_id,
                run_id: by.run_id,
                kind: "embedding",
                provider: embedder.provider.id().to_string(),
                model: embedder.model.clone(),
                input_tokens: i32::try_from(usage.input_tokens).unwrap_or(i32::MAX),
                output_tokens: 0,
                cost_micros: self
                    .prices
                    .cost_micros(&embedder.model, &usage)
                    .unwrap_or(0),
            },
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
