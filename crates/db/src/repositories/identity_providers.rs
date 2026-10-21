//! The identity providers a company signs in with (ADR-063) and the e-mail
//! domains each covers, one provider per domain within the company.
//! `find_identity_provider` is the only read across tenants (a SECURITY
//! DEFINER function, before sign-in); the rest runs in the company's tenant.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// What a provider is (`identity_providers_kind_check`).
pub const IDENTITY_PROVIDER_KINDS: [&str; 3] = ["microsoft", "google", "oidc"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdentityProviderRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub kind: String,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub enabled: bool,
    /// In alphabetical order.
    pub domains: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SELECT: &str = "select p.id, p.tenant_id, p.kind, p.name, p.issuer, p.client_id, \
    p.enabled, coalesce(array(select d.domain::text from identity_provider_domains d \
    where d.provider_id = p.id order by 1), '{}') as domains, p.created_at, p.updated_at \
    from identity_providers p";

pub async fn list_identity_providers(
    conn: &mut PgConnection,
) -> sqlx::Result<Vec<IdentityProviderRow>> {
    sqlx::query_as(&format!("{SELECT} order by p.created_at, p.id"))
        .fetch_all(conn)
        .await
}

pub async fn get_identity_provider(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<IdentityProviderRow>> {
    sqlx::query_as(&format!("{SELECT} where p.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

pub struct NewIdentityProvider<'a> {
    pub tenant_id: Uuid,
    pub kind: &'a str,
    pub name: &'a str,
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub enabled: bool,
    pub created_by_user_id: Uuid,
}

/// `None` when the company already declared this client of this issuer.
pub async fn create_identity_provider(
    conn: &mut PgConnection,
    input: NewIdentityProvider<'_>,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "insert into identity_providers \
         (tenant_id, kind, name, issuer, client_id, enabled, created_by_user_id) \
         values ($1, $2, $3, $4, $5, $6, $7) \
         on conflict (tenant_id, issuer, client_id) do nothing returning id",
    )
    .bind(input.tenant_id)
    .bind(input.kind)
    .bind(input.name)
    .bind(input.issuer)
    .bind(input.client_id)
    .bind(input.enabled)
    .bind(input.created_by_user_id)
    .fetch_optional(conn)
    .await
}

/// Fields left to `None` are unchanged; whether the provider exists.
#[derive(Default)]
pub struct IdentityProviderPatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
}

pub async fn update_identity_provider(
    conn: &mut PgConnection,
    id: Uuid,
    patch: IdentityProviderPatch,
) -> sqlx::Result<bool> {
    let updated = sqlx::query(
        "update identity_providers set name = coalesce($2, name), \
         enabled = coalesce($3, enabled), updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(patch.name)
    .bind(patch.enabled)
    .execute(conn)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Replaces the domains the provider covers; the first one another provider
/// of the company covers already, if any: the caller then rolls back.
pub async fn replace_provider_domains(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    provider_id: Uuid,
    domains: &[String],
) -> sqlx::Result<Option<String>> {
    sqlx::query("delete from identity_provider_domains where provider_id = $1")
        .bind(provider_id)
        .execute(&mut *conn)
        .await?;
    for domain in domains {
        let inserted = sqlx::query(
            "insert into identity_provider_domains (domain, tenant_id, provider_id) \
             values ($1, $2, $3) on conflict (tenant_id, domain) do nothing",
        )
        .bind(domain)
        .bind(tenant_id)
        .bind(provider_id)
        .execute(&mut *conn)
        .await?;
        if inserted.rows_affected() == 0 {
            return Ok(Some(domain.clone()));
        }
    }
    Ok(None)
}

/// With its domains and the identities tied to it; whether it existed.
pub async fn delete_identity_provider(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let deleted = sqlx::query("delete from identity_providers where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() == 1)
}

/// The provider an address leads to before sign-in.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProviderLookup {
    pub tenant_id: Uuid,
    pub provider_id: Uuid,
    pub kind: String,
    pub name: String,
}

/// The enabled provider covering the domain of the account with this
/// address, active or invited, in an active company.
pub async fn find_identity_provider(
    pool: &PgPool,
    email: &str,
) -> sqlx::Result<Option<ProviderLookup>> {
    sqlx::query_as("select tenant_id, provider_id, kind, name from auth_find_identity_provider($1)")
        .bind(email)
        .fetch_optional(pool)
        .await
}
