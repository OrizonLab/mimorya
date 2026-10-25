//! What the agent finds and reads in a company's documents (ADR-096): their
//! passages searched by their words and, when the company chose an
//! embedding model, by their meaning, the two rankings fused by reciprocal
//! rank as memory's are (`memory_search`); and a document's passages read in
//! their order. Only the documents the reader's filter reaches are searched
//! (`VISIBLE`, in SQL, before anything is compared): a passage out of reach
//! is never ranked, its vector compared exactly over the others.

use mimorya_core::documents::DocumentReadFilter;
use sqlx::PgConnection;
use uuid::Uuid;

use super::documents::{VISIBLE, bind_visible};
use super::memory_embeddings::vector_literal;
use super::memory_search::{Meaning, RANK_CONSTANT, to_or_tsquery};

/// A passage found, with its document's title.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PassageHit {
    pub document_id: Uuid,
    pub title: String,
    pub ordinal: i32,
    pub page: Option<i32>,
    pub heading: Option<String>,
    pub content: String,
    /// Fused rank: higher is nearer.
    pub score: f32,
}

/// The passages of the documents `filter` reads that best match `query`, by
/// its words and, given its `meaning`, by its sense; at most `limit`.
pub async fn search_documents(
    conn: &mut PgConnection,
    filter: &DocumentReadFilter,
    query: &str,
    meaning: Option<Meaning<'_>>,
    limit: i64,
) -> sqlx::Result<Vec<PassageHit>> {
    let ts_query = to_or_tsquery(query);
    if ts_query.is_none() && meaning.is_none() {
        return Ok(Vec::new());
    }
    // Each ranking offers more than kept, for the fusion to choose from.
    let candidates = (limit * 4).clamp(1, 100);
    let sql = format!(
        "with visible as materialized ( \
           select id, title, created_at from documents where {VISIBLE}), \
         words as ( \
           select c.id, row_number() over (order by ts_rank_cd(c.content_tsv, \
             to_tsquery('french', $5)) desc, v.created_at desc, c.document_id, c.ordinal) as rank \
           from document_chunks c join visible v on v.id = c.document_id \
           where $5::text is not null and c.content_tsv @@ to_tsquery('french', $5) \
           order by rank limit $6), \
         near as materialized ( \
           select c.id, c.document_id, c.ordinal, v.created_at, \
             1 - (c.embedding <=> $7::vector) as similarity \
           from document_chunks c join visible v on v.id = c.document_id \
           where $7::text is not null and c.embedding is not null and c.embedding_model = $8), \
         meaning as ( \
           select id, row_number() over (order by similarity desc, created_at desc, \
             document_id, ordinal) as rank \
           from near where similarity >= $9 and similarity <> 'NaN' \
           order by rank limit $6) \
         select c.document_id, v.title, c.ordinal, c.page, c.heading, c.content, \
           (coalesce(1.0 / ({RANK_CONSTANT} + words.rank), 0) \
            + coalesce(1.0 / ({RANK_CONSTANT} + meaning.rank), 0))::real as score \
         from words full join meaning on meaning.id = words.id \
         join document_chunks c on c.id = coalesce(words.id, meaning.id) \
         join visible v on v.id = c.document_id \
         order by score desc, v.created_at desc, c.document_id, c.ordinal limit $10"
    );
    bind_visible(sqlx::query_as(&sql), filter)
        .bind(ts_query)
        .bind(candidates)
        .bind(meaning.map(|m| vector_literal(m.vector)))
        .bind(meaning.map(|m| m.model))
        .bind(meaning.map_or(1.0, |m| f64::from(m.min_similarity)))
        .bind(limit)
        .fetch_all(conn)
        .await
}

/// A passage read in its document.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PassageRow {
    pub ordinal: i32,
    pub page: Option<i32>,
    pub heading: Option<String>,
    pub content: String,
}

/// The passages of the document `id` from the `from`-th, in order; at most
/// `count`. The reader's right to read it is checked first
/// (`get_visible_document`).
pub async fn document_passages(
    conn: &mut PgConnection,
    id: Uuid,
    from: i32,
    count: i64,
) -> sqlx::Result<Vec<PassageRow>> {
    sqlx::query_as(
        "select ordinal, page, heading, content from document_chunks \
         where document_id = $1 and ordinal >= $2 order by ordinal limit $3",
    )
    .bind(id)
    .bind(from)
    .bind(count)
    .fetch_all(conn)
    .await
}
