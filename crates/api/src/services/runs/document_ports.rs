//! The documents a run reads, backed by PostgreSQL (ADR-096): passages found
//! by their words and, when the company chose an embedding model, their
//! meaning; a document's passages read in order once its reader's filter is
//! checked. Only the documents the filter reaches are ever read.

use async_trait::async_trait;
use mimorya_agent::runtime::{DocumentExcerpt, DocumentHit, DocumentPassage, DocumentStore};
use mimorya_core::documents::DocumentReadFilter;
use mimorya_db::repositories::document_search::{
    PassageHit, PassageRow, document_passages, search_documents,
};
use mimorya_db::repositories::documents::{count_passages, get_visible_document};
use uuid::Uuid;

use super::db_ports::DbPorts;
use crate::services::embeddings::UsageBy;

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

#[async_trait]
impl DocumentStore for DbPorts {
    async fn search(
        &self,
        filter: &DocumentReadFilter,
        query: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<DocumentHit>> {
        let by = UsageBy {
            user_id: Some(self.scope.user_id),
            run_id: Some(self.scope.run_id),
        };
        let tenant_id = self.scope.tenant_id;
        let meaning = self.embeddings.meaning_of(tenant_id, query, by).await;
        let meaning = meaning.as_ref().map(|m| m.as_meaning());
        let mut tx = self.tx().await?;
        let hits = search_documents(&mut tx, filter, query, meaning, i64::from(limit)).await?;
        tx.commit().await?;
        Ok(hits.into_iter().map(hit).collect())
    }

    async fn read(
        &self,
        filter: &DocumentReadFilter,
        document_id: Uuid,
        from: u32,
        count: u32,
    ) -> anyhow::Result<Option<DocumentExcerpt>> {
        let mut tx = self.tx().await?;
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
