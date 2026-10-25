//! The vectors of a company's document passages (ADR-096), as of its
//! memory's (`memory_embeddings`): the passages awaiting one from the model
//! the company chose, storing it, and wiping them all when the company stops
//! searching by meaning. A vector is derived from its passage, and deleted
//! with it.

use sqlx::PgConnection;
use uuid::Uuid;

use super::memory_embeddings::{IndexProgress, vector_literal};

/// What a passage's vector is computed from: its document's title, its
/// section's, and its text.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PassageToEmbed {
    pub id: Uuid,
    pub title: String,
    pub heading: Option<String>,
    pub content: String,
}

/// Passages not yet given to `model`, the oldest documents first, each in
/// its order.
pub async fn passages_to_embed(
    conn: &mut PgConnection,
    model: &str,
    limit: i64,
) -> sqlx::Result<Vec<PassageToEmbed>> {
    sqlx::query_as(
        "select c.id, d.title, c.heading, c.content \
         from document_chunks c join documents d on d.id = c.document_id \
         where c.embedding_model is distinct from $1 \
         order by d.created_at, d.id, c.ordinal limit $2",
    )
    .bind(model)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// Stores the vector `model` gave a passage, `None` when the model refused
/// its text: then it is not asked again, and the passage is found by its
/// words only. A passage deleted meanwhile is left alone.
pub async fn store_passage_embedding(
    conn: &mut PgConnection,
    id: Uuid,
    vector: Option<&[f32]>,
    model: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "update document_chunks set embedding = $2::vector, embedding_model = $3 where id = $1",
    )
    .bind(id)
    .bind(vector.map(vector_literal))
    .bind(model)
    .execute(conn)
    .await?;
    Ok(())
}

/// Wipes every passage vector of the company; how many passages had one.
pub async fn clear_passage_embeddings(conn: &mut PgConnection) -> sqlx::Result<u64> {
    let result = sqlx::query(
        "update document_chunks set embedding = null, embedding_model = null \
         where embedding_model is not null",
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected())
}

/// How far the company's passages are given to `model`: how many have its
/// vector, and how many there are.
pub async fn passage_index_progress(
    conn: &mut PgConnection,
    model: &str,
) -> sqlx::Result<IndexProgress> {
    sqlx::query_as(
        "select count(*) filter (where embedding_model = $1 and embedding is not null) \
         as embedded, count(*) as total from document_chunks",
    )
    .bind(model)
    .fetch_one(conn)
    .await
}

/// Whether the company's passages await vectors of `model`.
pub async fn passages_await_embeddings(conn: &mut PgConnection, model: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select exists (select 1 from document_chunks where embedding_model is distinct from $1)",
    )
    .bind(model)
    .fetch_one(conn)
    .await
}
