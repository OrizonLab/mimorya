//! The documents a run reads (ADR-096): the passages found for a request,
//! and those of one document in their order, where the user's filter
//! reaches only. The API keeps them in PostgreSQL, `testing` in memory.

use async_trait::async_trait;
use mimorya_core::documents::DocumentReadFilter;
use uuid::Uuid;

/// A passage of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPassage {
    /// Its position in its document, from 0.
    pub ordinal: u32,
    /// Its page, from 1, when the document has pages.
    pub page: Option<u32>,
    /// The title of its section.
    pub heading: Option<String>,
    pub content: String,
}

/// A passage found, with its document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentHit {
    pub document_id: Uuid,
    pub title: String,
    pub passage: DocumentPassage,
}

/// Passages of one document, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentExcerpt {
    pub title: String,
    pub passages: Vec<DocumentPassage>,
    /// How many passages the document holds.
    pub total: u32,
}

#[async_trait]
pub trait DocumentStore: Send + Sync {
    /// The passages that best match `query`, searched only within `filter`
    /// (pre-filtering, never post-filtering); at most `limit`.
    async fn search(
        &self,
        filter: &DocumentReadFilter,
        query: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<DocumentHit>>;

    /// The passages of `document_id` from the `from`-th, at most `count`;
    /// `None` when `filter` does not read it, as when it does not exist.
    async fn read(
        &self,
        filter: &DocumentReadFilter,
        document_id: Uuid,
        from: u32,
        count: u32,
    ) -> anyhow::Result<Option<DocumentExcerpt>>;
}
