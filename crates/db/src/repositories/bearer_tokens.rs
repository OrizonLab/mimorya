//! The tokens another system holds of a company and sends as
//! `Authorization: Bearer`: its SIEM reading the audit log (ADR-181), its
//! identity provider provisioning accounts through SCIM (ADR-183). Each kind
//! has its table; all are made, listed and removed in the company's
//! transaction, never their digest read back; the company of a token found
//! before any session, by a narrowly scoped database function, and when it
//! was last used.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A kind of token, and where it is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenTable {
    /// A SIEM reading the audit log (ADR-181).
    AuditExport,
    /// An identity provider provisioning accounts (ADR-183).
    Scim,
}

impl TokenTable {
    fn table(self) -> &'static str {
        match self {
            Self::AuditExport => "audit_export_tokens",
            Self::Scim => "scim_tokens",
        }
    }

    /// The function that finds a token's company before any session.
    fn lookup(self) -> &'static str {
        match self {
            Self::AuditExport => "auth_audit_export_token",
            Self::Scim => "auth_scim_token",
        }
    }
}

/// A token as the administration lists it: never its digest.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct BearerTokenRow {
    pub id: Uuid,
    pub name: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

const COLUMNS: &str = "id, name, created_by, created_at, last_used_at";

/// The company's tokens of `kind`, the oldest first.
pub async fn list_tokens(
    conn: &mut PgConnection,
    kind: TokenTable,
) -> sqlx::Result<Vec<BearerTokenRow>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from {} order by created_at, id",
        kind.table()
    ))
    .fetch_all(conn)
    .await
}

/// How many tokens of `kind` the company keeps.
pub async fn count_tokens(conn: &mut PgConnection, kind: TokenTable) -> sqlx::Result<i64> {
    sqlx::query_scalar(&format!("select count(*) from {}", kind.table()))
        .fetch_one(conn)
        .await
}

/// A token of `kind` of the company `tenant_id`, kept by its digest alone.
pub async fn create_token(
    conn: &mut PgConnection,
    kind: TokenTable,
    tenant_id: Uuid,
    name: &str,
    token_hash: &str,
    created_by: Uuid,
) -> sqlx::Result<BearerTokenRow> {
    sqlx::query_as(&format!(
        "insert into {} (tenant_id, name, token_hash, created_by) \
         values ($1, $2, $3, $4) returning {COLUMNS}",
        kind.table()
    ))
    .bind(tenant_id)
    .bind(name)
    .bind(token_hash)
    .bind(created_by)
    .fetch_one(conn)
    .await
}

/// Removes the company's token `id` of `kind`; what it was, if it was there.
pub async fn delete_token(
    conn: &mut PgConnection,
    kind: TokenTable,
    id: Uuid,
) -> sqlx::Result<Option<BearerTokenRow>> {
    sqlx::query_as(&format!(
        "delete from {} where id = $1 returning {COLUMNS}",
        kind.table()
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// The company and the token of `kind` whose digest is `token_hash`, if that
/// company is active: before any session, through a narrowly scoped function.
pub async fn token_tenant(
    pool: &PgPool,
    kind: TokenTable,
    token_hash: &str,
) -> sqlx::Result<Option<(Uuid, Uuid)>> {
    sqlx::query_as(&format!(
        "select tenant_id, token_id from {}($1)",
        kind.lookup()
    ))
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

/// Notes that the company's token `id` of `kind` was used at `now`; whether
/// it is still there. A removal under way is waited for, and seen.
pub async fn touch_token(
    conn: &mut PgConnection,
    kind: TokenTable,
    id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<bool> {
    let touched = sqlx::query(&format!(
        "update {} set last_used_at = $2 where id = $1",
        kind.table()
    ))
    .bind(id)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(touched.rows_affected() == 1)
}
