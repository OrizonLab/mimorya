//! The model providers a company chose (ADR-024): one for its conversations,
//! one for reviewing their answers (ADR-108); each API key is a secret.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// What a company's model serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPurpose {
    /// Its runs: conversations, missions, scheduled tasks, evaluations.
    Conversations,
    /// The review of their answers (ADR-108).
    Verification,
}

impl ModelPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelPurpose::Conversations => "conversations",
            ModelPurpose::Verification => "verification",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LlmSettingsRow {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub async fn get_llm_settings(
    conn: &mut PgConnection,
    purpose: ModelPurpose,
) -> sqlx::Result<Option<LlmSettingsRow>> {
    sqlx::query_as(
        "select provider, model, base_url, updated_at from tenant_llm_settings \
         where purpose = $1",
    )
    .bind(purpose.as_str())
    .fetch_optional(conn)
    .await
}

pub struct NewLlmSettings<'a> {
    pub tenant_id: Uuid,
    pub purpose: ModelPurpose,
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
        "insert into tenant_llm_settings \
         (tenant_id, purpose, provider, model, base_url, updated_by_user_id) \
         values ($1, $2, $3, $4, $5, $6) \
         on conflict (tenant_id, purpose) do update set provider = excluded.provider, \
         model = excluded.model, base_url = excluded.base_url, \
         updated_by_user_id = excluded.updated_by_user_id, updated_at = now()",
    )
    .bind(settings.tenant_id)
    .bind(settings.purpose.as_str())
    .bind(settings.provider)
    .bind(settings.model)
    .bind(settings.base_url)
    .bind(settings.updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns whether the company had settings of its own for `purpose`.
pub async fn delete_llm_settings(
    conn: &mut PgConnection,
    purpose: ModelPurpose,
) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from tenant_llm_settings where purpose = $1")
        .bind(purpose.as_str())
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
