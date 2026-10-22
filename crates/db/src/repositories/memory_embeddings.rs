//! The vectors of a company's memory (ADR-074): the memories awaiting one
//! from the model the company chose, storing it, and wiping them all when
//! the company stops searching by meaning. A vector is derived from its
//! memory, and deleted with it.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// What a memory's vector is computed from.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemoryToEmbed {
    pub id: Uuid,
    pub title: String,
    pub content: String,
}

/// Active memories not yet given to `model`, oldest first: a proposal is
/// sent to no model before it is approved.
pub async fn memories_to_embed(
    conn: &mut PgConnection,
    model: &str,
    limit: i64,
) -> sqlx::Result<Vec<MemoryToEmbed>> {
    sqlx::query_as(
        "select id, title, content from memory_items \
         where status = 'active' and embedding_model is distinct from $1 \
         order by created_at, id limit $2",
    )
    .bind(model)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// pgvector's text form of a vector: `[0.1,0.2,…]`.
pub fn vector_literal(values: &[f32]) -> String {
    let values: Vec<String> = values.iter().map(f32::to_string).collect();
    format!("[{}]", values.join(","))
}

/// Stores the vector `model` gave a memory, `None` when the model refused
/// its text: then it is not asked again, and the memory is found by its
/// words only. A memory deleted meanwhile is left alone.
pub async fn store_embedding(
    conn: &mut PgConnection,
    id: Uuid,
    vector: Option<&[f32]>,
    model: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "update memory_items set embedding = $2::vector, embedding_model = $3 where id = $1",
    )
    .bind(id)
    .bind(vector.map(vector_literal))
    .bind(model)
    .execute(conn)
    .await?;
    Ok(())
}

/// Wipes every vector of the company; how many memories had one.
pub async fn clear_embeddings(conn: &mut PgConnection) -> sqlx::Result<u64> {
    let result = sqlx::query(
        "update memory_items set embedding = null, embedding_model = null \
         where embedding_model is not null",
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected())
}

/// How far the company's active memory is given to `model`: how many
/// memories have its vector, and how many there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct IndexProgress {
    pub embedded: i64,
    pub total: i64,
}

pub async fn index_progress(conn: &mut PgConnection, model: &str) -> sqlx::Result<IndexProgress> {
    sqlx::query_as(
        "select count(*) filter (where embedding_model = $1 and embedding is not null) \
         as embedded, count(*) as total \
         from memory_items where status = 'active'",
    )
    .bind(model)
    .fetch_one(conn)
    .await
}

/// The active companies whose memory awaits vectors of the model they chose.
pub async fn tenants_awaiting_embeddings(pool: &PgPool) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("select system_tenants_awaiting_embeddings()")
        .fetch_all(pool)
        .await
}
