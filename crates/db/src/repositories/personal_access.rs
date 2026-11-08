//! How one person reaches the platform, as the export of their own data
//! reads it (ADR-187): their sessions (when, from which address, with what
//! browser; never the token's hash), the identity providers they sign in
//! through and who those say they are, and the identifier their company's
//! provider gave them through SCIM. Read in the person's company's
//! transaction, for the person alone.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedSession {
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

/// An identity provider the person signs in through (ADR-063).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedSignIn {
    pub provider: String,
    pub kind: String,
    /// Who the provider says the person is.
    pub subject: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

/// The person's account as the company's identity provider made it through
/// SCIM (ADR-183).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedProvisioning {
    pub external_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub removed_at: Option<DateTime<Utc>>,
}

pub async fn exported_sessions(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedSession>> {
    sqlx::query_as(
        "select created_at, last_seen_at, expires_at, revoked_at, host(ip) as ip, user_agent \
         from sessions where user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_sign_ins(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedSignIn>> {
    sqlx::query_as(
        "select p.name as provider, p.kind, i.subject, i.created_at, i.last_used_at \
         from user_identities i \
         join identity_providers p on p.tenant_id = i.tenant_id and p.id = i.provider_id \
         where i.user_id = $1 order by i.created_at, i.id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_provisioning(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedProvisioning>> {
    sqlx::query_as(
        "select external_id, created_at, removed_at from scim_accounts \
         where user_id = $1 order by created_at, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}
