//! The model provider a company chose (ADR-024); its API key is a secret.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LlmSettingsRow {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub async fn get_llm_settings(conn: &mut PgConnection) -> sqlx::Result<Option<LlmSettingsRow>> {
    sqlx::query_as("select provider, model, base_url, updated_at from tenant_llm_settings")
        .fetch_optional(conn)
        .await
}

pub struct NewLlmSettings<'a> {
    pub tenant_id: Uuid,
    pub provider: &'a str,
    pub model: &'a str,
    pub base_url: Option<&'a str>,
    pub updated_by_user_id: Uuid,
}

pub async fn put_llm_settings(
    conn: &mut PgConnection,
    settings: NewLlmSettings<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_llm_settings (tenant_id, provider, model, base_url, updated_by_user_id) \
         values ($1, $2, $3, $4, $5) \
         on conflict (tenant_id) do update set provider = excluded.provider, \
         model = excluded.model, base_url = excluded.base_url, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(settings.tenant_id)
    .bind(settings.provider)
    .bind(settings.model)
    .bind(settings.base_url)
    .bind(settings.updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns whether the company had settings of its own.
pub async fn delete_llm_settings(conn: &mut PgConnection) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from tenant_llm_settings")
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
