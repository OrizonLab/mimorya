//! What the agent reads of the company's memory and documents (ADR-074,
//! ADR-096), for a run's ports and for an evaluation's (ADR-104): found by
//! their words and, when the company chose an embedding model, their
//! meaning, only where the reader's filter reaches.

use mimorya_agent::runtime::{DocumentExcerpt, DocumentHit, DocumentPassage, MemoryHit};
use mimorya_core::authz::Sensitivity;
use mimorya_core::documents::DocumentReadFilter;
use mimorya_core::memory::MemoryReadFilter;
use mimorya_db::Database;
use mimorya_db::repositories::document_search::{
    PassageHit, PassageRow, document_passages, search_documents,
};
use mimorya_db::repositories::documents::{count_passages, get_visible_document};
use mimorya_db::repositories::memory::search_memory;
use uuid::Uuid;

use super::context::memory_hit;
use crate::services::embeddings::{EmbeddingService, UsageBy};

/// Who reads, in which company: what the vector of a request costs is
/// counted for them.
pub struct Reader<'a> {
    pub db: &'a Database,
    pub embeddings: &'a EmbeddingService,
    pub tenant_id: Uuid,
    pub by: UsageBy,
}

impl Reader<'_> {
    pub async fn search_memory(
        &self,
        filter: &MemoryReadFilter,
        query: &str,
        limit: u32,
        clearance: Sensitivity,
    ) -> anyhow::Result<Vec<MemoryHit>> {
        let meaning = self
            .embeddings
            .meaning_of(self.tenant_id, query, self.by)
            .await;
        let meaning = meaning.as_ref().map(|m| m.as_meaning());
        let mut tx = self.db.begin(self.tenant_id).await?;
        let limit = i64::from(limit);
        let hits = search_memory(&mut tx, filter, query, meaning, limit, clearance).await?;
        tx.commit().await?;
        Ok(hits.into_iter().filter_map(memory_hit).collect())
    }

    pub async fn search_documents(
        &self,
        filter: &DocumentReadFilter,
        query: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<DocumentHit>> {
        let meaning = self
            .embeddings
            .meaning_of(self.tenant_id, query, self.by)
            .await;
        let meaning = meaning.as_ref().map(|m| m.as_meaning());
        let mut tx = self.db.begin(self.tenant_id).await?;
        let hits = search_documents(&mut tx, filter, query, meaning, i64::from(limit)).await?;
        tx.commit().await?;
        Ok(hits.into_iter().map(hit).collect())
    }

    /// `count` passages of the document, from the `from`-th, once the
    /// filter reaches it; `None` when it does not, or it is absent.
    pub async fn read_document(
        &self,
        filter: &DocumentReadFilter,
        document_id: Uuid,
        from: u32,
        count: u32,
    ) -> anyhow::Result<Option<DocumentExcerpt>> {
        let mut tx = self.db.begin(self.tenant_id).await?;
        let Some(document) = get_visible_document(&mut tx, filter, document_id).await? else {
            return Ok(None);
        };
        let from = i32::try_from(from).unwrap_or(i32::MAX);
        let passages = document_passages(&mut tx, document_id, from, i64::from(count)).await?;
        let total = count_passages(&mut tx, document_id).await?;
        tx.commit().await?;
        Ok(Some(DocumentExcerpt {
            title: document.title,
            passages: passages.into_iter().map(read).collect(),
            total: u32::try_from(total).unwrap_or(u32::MAX),
        }))
    }
}

fn passage(
    ordinal: i32,
    page: Option<i32>,
    heading: Option<String>,
    content: String,
) -> DocumentPassage {
    DocumentPassage {
        ordinal: u32::try_from(ordinal).unwrap_or_default(),
        page: page.and_then(|page| u32::try_from(page).ok()),
        heading,
        content,
    }
}

fn hit(row: PassageHit) -> DocumentHit {
    DocumentHit {
        document_id: row.document_id,
        title: row.title,
        passage: passage(row.ordinal, row.page, row.heading, row.content),
    }
}

fn read(row: PassageRow) -> DocumentPassage {
    passage(row.ordinal, row.page, row.heading, row.content)
}
