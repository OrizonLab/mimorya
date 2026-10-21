//! The identity providers a company signs in with (ADR-063) and the e-mail
//! domains each covers, one provider per domain within the company.
//! `find_identity_provider` and `find_provider_link` are the only reads
//! across tenants (SECURITY DEFINER functions, before sign-in); the rest
//! runs in the company's tenant.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdentityProviderRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub kind: String,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub enabled: bool,
    /// Its domains' accounts sign in through it alone (ADR-066).
    pub required: bool,
    /// It creates the account of a person of its domains at their first
    /// sign-in through the company's link (ADR-067).
    pub create_accounts: bool,
    /// SAML (ADR-068): the provider's metadata, as declared.
    pub saml_metadata: Option<String>,
    /// In byte order, whatever the database's collation.
    pub domains: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SELECT: &str = "select p.id, p.tenant_id, p.kind, p.name, p.issuer, p.client_id, \
    p.enabled, p.required, p.create_accounts, p.saml_metadata, coalesce(array(select d.domain::text from identity_provider_domains d \
    where d.provider_id = p.id order by d.domain::text collate \"C\"), '{}') as domains, \
    p.created_at, p.updated_at \
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
    pub required: bool,
    pub create_accounts: bool,
    pub saml_metadata: Option<&'a str>,
    pub created_by_user_id: Uuid,
}

/// `None` when the company already declared this client of this issuer.
pub async fn create_identity_provider(
    conn: &mut PgConnection,
    input: NewIdentityProvider<'_>,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "insert into identity_providers \
         (tenant_id, kind, name, issuer, client_id, enabled, required, create_accounts, \
         saml_metadata, created_by_user_id) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         on conflict (tenant_id, issuer, client_id) do nothing returning id",
    )
    .bind(input.tenant_id)
    .bind(input.kind)
    .bind(input.name)
    .bind(input.issuer)
    .bind(input.client_id)
    .bind(input.enabled)
    .bind(input.required)
    .bind(input.create_accounts)
    .bind(input.saml_metadata)
    .bind(input.created_by_user_id)
    .fetch_optional(conn)
    .await
}

/// Fields left to `None` are unchanged; whether the provider exists.
#[derive(Default)]
pub struct IdentityProviderPatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub required: Option<bool>,
    pub create_accounts: Option<bool>,
    /// SAML: the provider's metadata anew (its certificates rolled over).
    pub saml_metadata: Option<String>,
}

pub async fn update_identity_provider(
    conn: &mut PgConnection,
    id: Uuid,
    patch: IdentityProviderPatch,
) -> sqlx::Result<bool> {
    let updated = sqlx::query(
        "update identity_providers set name = coalesce($2, name), \
         enabled = coalesce($3, enabled), required = coalesce($4, required), \
         create_accounts = coalesce($5, create_accounts), \
         saml_metadata = coalesce($6, saml_metadata), updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(patch.name)
    .bind(patch.enabled)
    .bind(patch.required)
    .bind(patch.create_accounts)
    .bind(patch.saml_metadata)
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
    /// The account signs in through it alone (ADR-066).
    pub required: bool,
}

/// The enabled provider covering the domain of the account with this
/// address, active or invited (ADR-064), in an active company.
pub async fn find_identity_provider(
    pool: &PgPool,
    email: &str,
) -> sqlx::Result<Option<ProviderLookup>> {
    sqlx::query_as(
        "select tenant_id, provider_id, kind, name, required \
         from auth_find_identity_provider($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

/// The provider a company's sign-in link names (ADR-067), and the company.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProviderLink {
    pub tenant_id: Uuid,
    pub provider_id: Uuid,
    pub kind: String,
    pub name: String,
    pub required: bool,
    pub create_accounts: bool,
    /// The company's name, which the page shows.
    pub company: String,
}

/// The provider with this id, if enabled, in an active company.
pub async fn find_provider_link(
    pool: &PgPool,
    provider_id: Uuid,
) -> sqlx::Result<Option<ProviderLink>> {
    sqlx::query_as(
        "select tenant_id, provider_id, kind, name, required, create_accounts, company \
         from auth_find_identity_provider_link($1)",
    )
    .bind(provider_id)
    .fetch_optional(pool)
    .await
}

/// The name of the enabled provider required for the domain of this
/// account's address, in the transaction's company (ADR-066), if any.
pub async fn required_provider_of(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "select p.name from users u \
         join identity_provider_domains d on d.domain = split_part(u.email::text, '@', 2)::citext \
         join identity_providers p on p.id = d.provider_id \
         where u.id = $1 and p.enabled and p.required",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}
