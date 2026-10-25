//! The documents a run reads, backed by PostgreSQL (ADR-096): passages found
//! by their words and, when the company chose an embedding model, their
//! meaning; a document's passages read in order once its reader's filter is
//! checked. Only the documents the filter reaches are ever read
//! (`reads.rs`).

use async_trait::async_trait;
use mimorya_agent::runtime::{DocumentExcerpt, DocumentHit, DocumentStore};
use mimorya_core::documents::DocumentReadFilter;
use uuid::Uuid;

use super::db_ports::DbPorts;

#[async_trait]
impl DocumentStore for DbPorts {
    async fn search(
        &self,
        filter: &DocumentReadFilter,
        query: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<DocumentHit>> {
        self.reader().search_documents(filter, query, limit).await
    }

    async fn read(
        &self,
        filter: &DocumentReadFilter,
        document_id: Uuid,
        from: u32,
        count: u32,
    ) -> anyhow::Result<Option<DocumentExcerpt>> {
        self.reader()
            .read_document(filter, document_id, from, count)
            .await
    }
}
