//! Searching memory (ADR-004, ADR-074): by its words, and by meaning when
//! the company chose an embedding model, the two rankings fused by
//! reciprocal rank (RRF). Both run on the memories the subject may read
//! only (pre-filtering in SQL): a memory out of reach is never compared, let
//! alone ranked. The vectors are compared exactly, over these memories: an
//! approximate index would filter after the fact and miss some.

use chrono::{DateTime, Utc};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::MemoryReadFilter;
use sqlx::PgConnection;
use uuid::Uuid;

use super::memory::{ACTIVE, VISIBLE, bind_visibility};
use super::memory_embeddings::vector_literal;

/// RRF's constant: how much the first ranks weigh over the next.
pub(super) const RANK_CONSTANT: i32 = 60;

/// OR-query over the significant words of a natural-language request.
pub fn to_or_tsquery(query: &str) -> Option<String> {
    let mut words: Vec<String> = Vec::new();
    for word in query.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if word.chars().count() > 2 && !words.iter().any(|w| w == word) {
            words.push(word.to_string());
        }
        if words.len() == 24 {
            break;
        }
    }
    (!words.is_empty()).then(|| words.join(" | "))
}

/// A request's vector, to find memories by meaning too.
#[derive(Debug, Clone, Copy)]
pub struct Meaning<'a> {
    pub vector: &'a [f32],
    /// The model it comes from: only that model's vectors compare to it.
    pub model: &'a str,
    /// The cosine similarity a memory needs to count as close.
    pub min_similarity: f32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemorySearchHit {
    pub id: Uuid,
    pub scope: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub org_unit_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
    /// Fused rank: higher is nearer.
    pub score: f32,
}

/// The memories the subject may read that best match `query`, by its words
/// and, given its `meaning`, by its sense; at most `limit`.
pub async fn search_memory(
    conn: &mut PgConnection,
    filter: &MemoryReadFilter,
    query: &str,
    meaning: Option<Meaning<'_>>,
    limit: i64,
    clearance: Sensitivity,
) -> sqlx::Result<Vec<MemorySearchHit>> {
    let ts_query = to_or_tsquery(query);
    if ts_query.is_none() && meaning.is_none() {
        return Ok(Vec::new());
    }
    // Each ranking offers more than kept, for the fusion to choose from.
    let candidates = (limit * 4).clamp(1, 100);
    let sql = format!(
        "with words as ( \
           select id, row_number() over (order by ts_rank_cd(content_tsv, \
             to_tsquery('french', $8)) desc, updated_at desc, id) as rank \
           from memory_items where $8::text is not null and {ACTIVE} and {VISIBLE} \
             and content_tsv @@ to_tsquery('french', $8) \
           order by rank limit $9), \
         near as materialized ( \
           select id, updated_at, 1 - (embedding <=> $10::vector) as similarity \
           from memory_items where $10::text is not null and {ACTIVE} and {VISIBLE} \
             and embedding is not null and embedding_model = $11), \
         meaning as ( \
           select id, row_number() over (order by similarity desc, updated_at desc, id) as rank \
           from near where similarity >= $12 and similarity <> 'NaN' \
           order by rank limit $9) \
         select m.id, m.scope, m.kind, m.title, m.content, m.org_unit_id, m.updated_at, \
           (coalesce(1.0 / ({RANK_CONSTANT} + words.rank), 0) \
            + coalesce(1.0 / ({RANK_CONSTANT} + meaning.rank), 0))::real as score \
         from words full join meaning on meaning.id = words.id \
         join memory_items m on m.id = coalesce(words.id, meaning.id) \
         order by score desc, m.updated_at desc, m.id limit $13"
    );
    bind_visibility(sqlx::query_as(&sql), filter, clearance)
        .bind(ts_query)
        .bind(candidates)
        .bind(meaning.map(|m| vector_literal(m.vector)))
        .bind(meaning.map(|m| m.model))
        .bind(meaning.map_or(1.0, |m| f64::from(m.min_similarity)))
        .bind(limit)
        .fetch_all(conn)
        .await
}
