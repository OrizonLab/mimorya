//! The embedding model a company chose to search its memory by meaning
//! (ADR-074); its API key is a secret.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct EmbeddingSettingsRow {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub min_similarity: f32,
    pub updated_at: DateTime<Utc>,
}

pub async fn get_embedding_settings(
    conn: &mut PgConnection,
) -> sqlx::Result<Option<EmbeddingSettingsRow>> {
    sqlx::query_as(
        "select provider, model, base_url, min_similarity, updated_at \
         from tenant_embedding_settings",
    )
    .fetch_optional(conn)
    .await
}

/// The company's choice, locked until the transaction ends: a change, or
/// the return to searching by words, waits for it, and what is written
/// meanwhile follows the choice read.
pub async fn lock_embedding_settings(
    conn: &mut PgConnection,
) -> sqlx::Result<Option<EmbeddingSettingsRow>> {
    sqlx::query_as(
        "select provider, model, base_url, min_similarity, updated_at \
         from tenant_embedding_settings for share",
    )
    .fetch_optional(conn)
    .await
}

pub struct NewEmbeddingSettings<'a> {
    pub tenant_id: Uuid,
    pub provider: &'a str,
    pub model: &'a str,
    pub base_url: Option<&'a str>,
    pub min_similarity: f32,
    pub updated_by_user_id: Uuid,
}

pub async fn put_embedding_settings(
    conn: &mut PgConnection,
    settings: NewEmbeddingSettings<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_embedding_settings \
         (tenant_id, provider, model, base_url, min_similarity, updated_by_user_id) \
         values ($1, $2, $3, $4, $5, $6) \
         on conflict (tenant_id) do update set provider = excluded.provider, \
         model = excluded.model, base_url = excluded.base_url, \
         min_similarity = excluded.min_similarity, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(settings.tenant_id)
    .bind(settings.provider)
    .bind(settings.model)
    .bind(settings.base_url)
    .bind(settings.min_similarity)
    .bind(settings.updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns whether the company had chosen a model.
pub async fn delete_embedding_settings(conn: &mut PgConnection) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from tenant_embedding_settings")
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
