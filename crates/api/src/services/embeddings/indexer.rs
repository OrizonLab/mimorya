//! Giving a company's memory its vectors (ADR-074), in the background: the
//! active memories not yet embedded by the model the company chose (written
//! since, approved since, or embedded by another model before), a batch at
//! a time and a few batches per pass, so that one company's backlog never
//! holds the others'. A text the model refuses is not asked again (found by
//! its words only); any other failure is retried at the next pass, and
//! shown to the company's administrators until then.

use mimorya_agent::llm::LlmErrorKind;
use mimorya_agent::llm::embedding::MAX_EMBEDDING_BATCH;
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

impl EmbeddingService {
    /// Embeds the memories awaiting a vector in every company that chose a
    /// model; how many got one. One pass at a time.
    pub async fn index_pending(&self) -> anyhow::Result<usize> {
        let _pass = self.indexing.lock().await;
        let awaiting = tenants_awaiting_embeddings(self.db.pool()).await?;
        // A company with nothing left to embed has no failure left to show.
        self.failures
            .lock()
            .expect("indexing failures")
            .retain(|tenant_id, _| awaiting.contains(tenant_id));
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
            let batch_size = i64::try_from(MAX_EMBEDDING_BATCH)?;
            let batch = memories_to_embed(&mut tx, &embedder.model, batch_size).await?;
            tx.commit().await?;
            if batch.is_empty() {
                break;
            }
            let vectors = match self.embed(tenant_id, &embedder, &batch).await {
                Ok(vectors) => vectors,
                Err(reason) => {
                    self.failed(tenant_id, Some(reason.clone()));
                    anyhow::bail!(reason);
                }
            };
            let mut tx = self.db.begin(tenant_id).await?;
            for (memory, vector) in batch.iter().zip(&vectors) {
                store_embedding(&mut tx, memory.id, vector.as_deref(), &embedder.model).await?;
            }
            tx.commit().await?;
            embedded += vectors.iter().filter(|v| v.is_some()).count();
            if batch.len() < MAX_EMBEDDING_BATCH {
                break;
            }
        }
        self.failed(tenant_id, None);
        Ok(embedded)
    }

    /// The vector of each memory, `None` for a text the model refuses; why
    /// the batch could not be embedded otherwise (French).
    async fn embed(
        &self,
        tenant_id: Uuid,
        embedder: &CompanyEmbedder,
        batch: &[MemoryToEmbed],
    ) -> Result<Vec<Option<Vec<f32>>>, String> {
        let texts: Vec<String> = batch.iter().map(embedded_text).collect();
        match embedder.provider.embed(&texts).await {
            Ok(embedded) => {
                self.record(tenant_id, embedder, &embedded).await;
                Ok(embedded.vectors.into_iter().map(Some).collect())
            }
            // One of the texts is refused: each is asked alone.
            Err(error) if error.kind == LlmErrorKind::InvalidRequest && texts.len() > 1 => {
                let mut vectors = Vec::with_capacity(texts.len());
                for text in &texts {
                    vectors.push(self.embed_alone(tenant_id, embedder, text).await?);
                }
                Ok(vectors)
            }
            Err(error) if error.kind == LlmErrorKind::InvalidRequest => Ok(vec![None]),
            Err(error) => Err(error.message),
        }
    }

    async fn embed_alone(
        &self,
        tenant_id: Uuid,
        embedder: &CompanyEmbedder,
        text: &str,
    ) -> Result<Option<Vec<f32>>, String> {
        match embedder.provider.embed(&[text.to_string()]).await {
            Ok(embedded) => {
                self.record(tenant_id, embedder, &embedded).await;
                Ok(embedded.vectors.into_iter().next())
            }
            Err(error) if error.kind == LlmErrorKind::InvalidRequest => Ok(None),
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
