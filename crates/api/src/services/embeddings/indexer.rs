//! Giving a company's memory its vectors (ADR-074), in the background: the
//! active memories not yet embedded by the model the company chose (written
//! since, approved since, or embedded by another model before), a batch at
//! a time and a few batches per pass, so that one company's backlog never
//! holds the others'. The company's choice is read again before each batch
//! and locked while its vectors are written: once it changes, or the company
//! goes back to searching by words, nothing more is sent to the model, and
//! what the batch in flight brings back is dropped. A text the model refuses
//! as too long, or as invalid while others of its batch get their vector,
//! is not asked again (found by its words only); a batch whose every text
//! is refused as invalid is the model's failure, or its settings': retried
//! at the next pass, like any other failure, and shown to the company's
//! administrators until then.

use mimorya_agent::llm::embedding::MAX_EMBEDDING_BATCH;
use mimorya_agent::llm::{LlmError, LlmErrorKind};
use mimorya_db::repositories::embedding_settings::{
    get_embedding_settings, lock_embedding_settings, set_indexing_failure,
};
use mimorya_db::repositories::memory_embeddings::{
    MemoryToEmbed, memories_to_embed, store_embedding, tenants_awaiting_embeddings,
};
use uuid::Uuid;

use super::{CompanyEmbedder, EmbeddingService, UsageBy};

/// Batches embedded per company and pass.
const BATCHES_PER_PASS: usize = 8;

/// What a memory's vector is computed from.
fn embedded_text(memory: &MemoryToEmbed) -> String {
    format!("{}\n\n{}", memory.title, memory.content)
}

/// What became of a text asked alone.
enum Alone {
    Embedded(Vec<f32>),
    /// Refused, and how: too long (`ContextOverflow`) or invalid.
    Refused(LlmErrorKind),
}

/// The text sent is refused, not the request: invalid or too long for the
/// model.
fn refuses_text(error: &LlmError) -> bool {
    matches!(
        error.kind,
        LlmErrorKind::InvalidRequest | LlmErrorKind::ContextOverflow
    )
}

impl EmbeddingService {
    /// Embeds the memories awaiting a vector in every company that chose a
    /// model; how many got one. One pass at a time.
    pub async fn index_pending(&self) -> anyhow::Result<usize> {
        let _pass = self.indexing.lock().await;
        let awaiting = tenants_awaiting_embeddings(self.db.pool()).await?;
        let mut embedded = 0;
        for tenant_id in awaiting {
            match self.index_company(tenant_id).await {
                Ok(count) => embedded += count,
                Err(error) => {
                    tracing::warn!(%tenant_id, error = %error, "Memory indexing failed");
                }
            }
        }
        Ok(embedded)
    }

    async fn index_company(&self, tenant_id: Uuid) -> anyhow::Result<usize> {
        let Some(embedder) = self.embedder(tenant_id).await? else {
            return Ok(0);
        };
        let mut embedded = 0;
        for _ in 0..BATCHES_PER_PASS {
            let mut tx = self.db.begin(tenant_id).await?;
            let chosen = get_embedding_settings(&mut tx).await?;
            if !chosen.is_some_and(|settings| embedder.serves(&settings)) {
                // Changed meanwhile: the next pass follows the new choice.
                return Ok(embedded);
            }
            let batch_size = i64::try_from(MAX_EMBEDDING_BATCH)?;
            let batch = memories_to_embed(&mut tx, &embedder.model, batch_size).await?;
            tx.commit().await?;
            if batch.is_empty() {
                break;
            }
            let vectors = match self.embed(tenant_id, &embedder, &batch).await {
                Ok(vectors) => vectors,
                Err(reason) => {
                    self.record_outcome(tenant_id, &embedder, Some(&reason))
                        .await?;
                    anyhow::bail!(reason);
                }
            };
            let mut tx = self.db.begin(tenant_id).await?;
            let chosen = lock_embedding_settings(&mut tx).await?;
            if !chosen.is_some_and(|settings| embedder.serves(&settings)) {
                return Ok(embedded);
            }
            for (memory, vector) in batch.iter().zip(&vectors) {
                store_embedding(&mut tx, memory.id, vector.as_deref(), &embedder.model).await?;
            }
            tx.commit().await?;
            embedded += vectors.iter().filter(|v| v.is_some()).count();
            if batch.len() < MAX_EMBEDDING_BATCH {
                break;
            }
        }
        self.record_outcome(tenant_id, &embedder, None).await?;
        Ok(embedded)
    }

    /// Records why the company's pass failed (`None`: it succeeded), for its
    /// administrators, whatever process shows them, unless the choice it
    /// followed changed meanwhile.
    async fn record_outcome(
        &self,
        tenant_id: Uuid,
        embedder: &CompanyEmbedder,
        failure: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let chosen = lock_embedding_settings(&mut tx).await?;
        if chosen.is_some_and(|settings| embedder.serves(&settings)) {
            set_indexing_failure(&mut tx, failure).await?;
            tx.commit().await?;
        }
        Ok(())
    }

    /// The vector of each memory, `None` for a text the model refuses (too
    /// long, or invalid while it embeds others); why the batch could not be
    /// embedded otherwise (French).
    async fn embed(
        &self,
        tenant_id: Uuid,
        embedder: &CompanyEmbedder,
        batch: &[MemoryToEmbed],
    ) -> Result<Vec<Option<Vec<f32>>>, String> {
        let texts: Vec<String> = batch.iter().map(embedded_text).collect();
        let refusal = match embedder.provider.embed(&texts).await {
            Ok(embedded) => {
                self.record(tenant_id, embedder, &embedded).await;
                return Ok(embedded.vectors.into_iter().map(Some).collect());
            }
            Err(error) if refuses_text(&error) => error,
            Err(error) => return Err(error.message),
        };
        // Some text is refused: each is asked alone.
        let mut outcomes = Vec::with_capacity(texts.len());
        if texts.len() > 1 {
            for text in &texts {
                outcomes.push(self.embed_alone(tenant_id, embedder, text).await?);
            }
        } else {
            outcomes.push(Alone::Refused(refusal.kind));
        }
        // Too long is the text's fault; invalid, every one of them, the
        // model's or its settings'.
        let invalid = |o: &Alone| matches!(o, Alone::Refused(LlmErrorKind::InvalidRequest));
        if outcomes.iter().all(invalid) {
            return Err(refusal.message);
        }
        Ok(outcomes
            .into_iter()
            .map(|outcome| match outcome {
                Alone::Embedded(vector) => Some(vector),
                Alone::Refused(_) => None,
            })
            .collect())
    }

    async fn embed_alone(
        &self,
        tenant_id: Uuid,
        embedder: &CompanyEmbedder,
        text: &str,
    ) -> Result<Alone, String> {
        match embedder.provider.embed(&[text.to_string()]).await {
            Ok(embedded) => {
                self.record(tenant_id, embedder, &embedded).await;
                let vector = embedded.vectors.into_iter().next();
                Ok(vector.map_or(
                    Alone::Refused(LlmErrorKind::InvalidRequest),
                    Alone::Embedded,
                ))
            }
            Err(error) if refuses_text(&error) => Ok(Alone::Refused(error.kind)),
            Err(error) => Err(error.message),
        }
    }

    async fn record(
        &self,
        tenant_id: Uuid,
        embedder: &CompanyEmbedder,
        embedded: &mimorya_agent::llm::Embedded,
    ) {
        let by = UsageBy::default();
        if let Err(error) = self.record_usage(tenant_id, embedder, embedded, by).await {
            tracing::warn!(%tenant_id, error = %error, "Embedding usage not recorded");
        }
    }
}
