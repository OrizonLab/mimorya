//! What the platform keeps about one person in their company, as the export
//! of their own data reads it (ADR-187): their account, units and roles,
//! absences, and the accounts, messaging identities and servers they linked
//! or authorized. Never a password, a token nor a secret: only what says
//! who they are and what they set up. Read in the person's company's
//! transaction, for the person alone.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedProfile {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub status: String,
    pub clearance: String,
    pub attributes: Value,
    pub locale: String,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

/// A unit the person belongs to.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ExportedUnit {
    pub name: String,
    pub is_primary: bool,
}

/// A role given to the person, and over what.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedRole {
    pub name: String,
    pub scope_type: String,
    /// The unit it is given over, when it is one.
    pub scope_unit: Option<String>,
    pub include_descendants: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

/// An absence of the person, and who stood in.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedAbsence {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub delegate: String,
}

/// An account the person linked (ADR-116), without its token.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedLink {
    pub connector: String,
    /// Who the provider says the account is.
    pub subject: String,
    pub address: String,
    pub scopes: String,
    pub created_at: DateTime<Utc>,
}

/// A messaging identity the person linked (ADR-125).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedIdentity {
    pub kind: String,
    pub gateway: String,
    /// The person's identifier in the messaging service.
    pub external_id: String,
    pub display_name: Option<String>,
    pub address: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A server the person authorized for themself (ADR-174), without its
/// tokens.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedAuthorization {
    pub connector: String,
    pub scopes: String,
    pub authorized_at: DateTime<Utc>,
}

/// The person's account; `None` when the company holds no such account.
pub async fn exported_profile(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<ExportedProfile>> {
    sqlx::query_as(
        "select id, email::text as email, display_name, job_title, status, clearance, \
         attributes, locale, created_at, last_login_at from users where id = $1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

pub async fn exported_units(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedUnit>> {
    sqlx::query_as(
        "select u.name, m.is_primary from org_memberships m \
         join org_units u on u.tenant_id = m.tenant_id and u.id = m.org_unit_id \
         where m.user_id = $1 order by m.is_primary desc, u.name",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_roles(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedRole>> {
    sqlx::query_as(
        "select r.name, a.scope_type, u.name as scope_unit, a.include_descendants, a.expires_at \
         from role_assignments a \
         join roles r on r.tenant_id = a.tenant_id and r.id = a.role_id \
         left join org_units u on u.tenant_id = a.tenant_id and u.id = a.scope_org_unit_id \
         where a.user_id = $1 order by r.name, a.created_at",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_absences(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedAbsence>> {
    sqlx::query_as(
        "select a.starts_at, a.ends_at, d.display_name as delegate from approval_delegations a \
         join users d on d.tenant_id = a.tenant_id and d.id = a.delegate_user_id \
         where a.user_id = $1 order by a.starts_at",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_links(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedLink>> {
    sqlx::query_as(
        "select c.name as connector, l.subject, l.address, l.scopes, l.created_at \
         from account_links l \
         join connector_instances c on c.tenant_id = l.tenant_id and c.id = l.connector_id \
         where l.user_id = $1 order by l.created_at",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_identities(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedIdentity>> {
    sqlx::query_as(
        "select i.kind, g.name as gateway, i.external_id, i.display_name, i.address, i.created_at \
         from gateway_identities i \
         join gateways g on g.tenant_id = i.tenant_id and g.id = i.gateway_id \
         where i.user_id = $1 order by i.created_at",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

pub async fn exported_authorizations(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedAuthorization>> {
    sqlx::query_as(
        "select c.name as connector, a.scopes, a.authorized_at \
         from connector_person_authorizations a \
         join connector_instances c on c.tenant_id = a.tenant_id and c.id = a.connector_id \
         where a.user_id = $1 order by a.authorized_at",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}
