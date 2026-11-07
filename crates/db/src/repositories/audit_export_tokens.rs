//! The tokens with which a company's SIEM reads its audit log (ADR-181):
//! made, listed and removed in the company's transaction, never their
//! digest read back; the company of a token found before any session, by
//! a narrowly scoped database function, and when it was last used.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A token as the administration lists it: never its digest.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AuditExportTokenRow {
    pub id: Uuid,
    pub name: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

const COLUMNS: &str = "id, name, created_by, created_at, last_used_at";

/// The company's tokens, the oldest first.
pub async fn list_audit_export_tokens(
    conn: &mut PgConnection,
) -> sqlx::Result<Vec<AuditExportTokenRow>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from audit_export_tokens order by created_at, id"
    ))
    .fetch_all(conn)
    .await
}

/// How many tokens the company keeps.
pub async fn count_audit_export_tokens(conn: &mut PgConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar("select count(*) from audit_export_tokens")
        .fetch_one(conn)
        .await
}

/// A token of the company `tenant_id`, kept by its digest alone.
pub async fn create_audit_export_token(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    name: &str,
    token_hash: &str,
    created_by: Uuid,
) -> sqlx::Result<AuditExportTokenRow> {
    sqlx::query_as(&format!(
        "insert into audit_export_tokens (tenant_id, name, token_hash, created_by) \
         values ($1, $2, $3, $4) returning {COLUMNS}"
    ))
    .bind(tenant_id)
    .bind(name)
    .bind(token_hash)
    .bind(created_by)
    .fetch_one(conn)
    .await
}

/// Removes the company's token `id`; what it was, if it was there.
pub async fn delete_audit_export_token(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<AuditExportTokenRow>> {
    sqlx::query_as(&format!(
        "delete from audit_export_tokens where id = $1 returning {COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// The company and the token whose digest is `token_hash`, if that company
/// is active: before any session, through a narrowly scoped function.
pub async fn audit_export_token_tenant(
    pool: &PgPool,
    token_hash: &str,
) -> sqlx::Result<Option<(Uuid, Uuid)>> {
    sqlx::query_as("select tenant_id, token_id from auth_audit_export_token($1)")
        .bind(token_hash)
        .fetch_optional(pool)
        .await
}

/// Notes that the company's token `id` was used at `now`.
pub async fn touch_audit_export_token(
    conn: &mut PgConnection,
    id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query("update audit_export_tokens set last_used_at = $2 where id = $1")
        .bind(id)
        .bind(now)
        .execute(conn)
        .await?;
    Ok(())
}
