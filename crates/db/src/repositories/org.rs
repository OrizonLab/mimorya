//! Organisation, roles, policies and user access, as loaded for the
//! authorization engine (RLS-scoped to the transaction's tenant).

use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use mimorya_core::authz::{
    AbacPolicy, AssignmentScope, OrgUnit, OrgUnitType, PolicyEffect, RoleAssignment,
    RoleDefinition, Sensitivity, Subject,
};
use mimorya_core::knowledge_shares::KnowledgeShare;
use serde_json::{Map, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::knowledge_shares::load_knowledge_shares;

/// The order of a person's memberships, primary unit first: the unit their
/// access and the approvals made for them are checked in (ADR-059).
pub(super) const PRIMARY_UNIT_FIRST: &str = "is_primary desc, created_at";

#[derive(Debug, Clone)]
pub struct TenantRecord {
    pub id: Uuid,
    pub name: String,
    /// `active` or `suspended`.
    pub status: String,
    pub settings: Map<String, Value>,
}

#[derive(Debug, Clone)]
pub struct RoleRecord {
    pub definition: RoleDefinition,
    pub description: String,
    pub is_system: bool,
}

/// Everything the authorization engine needs for one tenant.
#[derive(Debug, Clone)]
pub struct AuthorizationData {
    pub tenant: TenantRecord,
    pub units: Vec<OrgUnit>,
    pub roles: Vec<RoleRecord>,
    pub policies: Vec<AbacPolicy>,
    /// The units' knowledge shared with others (ADR-143).
    pub knowledge_shares: Vec<KnowledgeShare>,
}

#[derive(sqlx::FromRow)]
struct TenantRow {
    id: Uuid,
    name: String,
    status: String,
    settings: Value,
}

#[derive(sqlx::FromRow)]
struct UnitRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    name: String,
    #[sqlx(rename = "type")]
    unit_type: String,
    lead_user_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct RoleRow {
    id: Uuid,
    key: String,
    name: String,
    description: String,
    permissions: Vec<String>,
    is_system: bool,
}

#[derive(sqlx::FromRow)]
struct PolicyRow {
    id: Uuid,
    description: String,
    effect: String,
    permissions: Vec<String>,
    condition: Value,
}

pub async fn load_authorization_data(conn: &mut PgConnection) -> anyhow::Result<AuthorizationData> {
    let tenant: TenantRow = sqlx::query_as("select id, name, status, settings from tenants")
        .fetch_optional(&mut *conn)
        .await?
        .context("Tenant context missing")?;
    let units = load_org_units(&mut *conn).await?;
    let roles: Vec<RoleRow> = sqlx::query_as(
        "select id, key, name, description, permissions, is_system from roles order by name",
    )
    .fetch_all(&mut *conn)
    .await?;
    let policies: Vec<PolicyRow> = sqlx::query_as(
        "select id, description, effect, permissions, condition from access_policies where enabled",
    )
    .fetch_all(&mut *conn)
    .await?;
    let knowledge_shares = load_knowledge_shares(&mut *conn).await?;

    Ok(AuthorizationData {
        tenant: TenantRecord {
            id: tenant.id,
            name: tenant.name,
            status: tenant.status,
            settings: match tenant.settings {
                Value::Object(map) => map,
                _ => Map::new(),
            },
        },
        units,
        roles: roles
            .into_iter()
            .map(|r| RoleRecord {
                definition: RoleDefinition {
                    id: r.id,
                    key: r.key,
                    name: r.name,
                    permissions: r.permissions,
                },
                description: r.description,
                is_system: r.is_system,
            })
            .collect(),
        policies: policies
            .into_iter()
            .map(|p| {
                Ok(AbacPolicy {
                    id: p.id,
                    description: p.description,
                    effect: if p.effect == "allow" {
                        PolicyEffect::Allow
                    } else {
                        PolicyEffect::Deny
                    },
                    permissions: p.permissions,
                    condition: serde_json::from_value(p.condition)
                        .with_context(|| format!("invalid condition in policy {}", p.id))?,
                })
            })
            .collect::<anyhow::Result<_>>()?,
        knowledge_shares,
    })
}

/// The organisation units of the tenant, as stored now.
pub async fn load_org_units(conn: &mut PgConnection) -> anyhow::Result<Vec<OrgUnit>> {
    let units: Vec<UnitRow> = sqlx::query_as(
        "select id, parent_id, name, type, lead_user_id from org_units order by created_at",
    )
    .fetch_all(conn)
    .await?;
    units
        .into_iter()
        .map(|u| {
            Ok(OrgUnit {
                id: u.id,
                parent_id: u.parent_id,
                name: u.name,
                unit_type: OrgUnitType::parse(&u.unit_type)
                    .ok_or_else(|| anyhow!("unknown org unit type {}", u.unit_type))?,
                lead_user_id: u.lead_user_id,
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct UserProfile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub status: String,
    pub locale: String,
    pub primary_unit_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct AssignmentRecord {
    pub assignment: RoleAssignment,
    pub role_name: String,
}

#[derive(Debug, Clone)]
pub struct UserAccess {
    pub profile: UserProfile,
    pub subject: Subject,
    pub assignments: Vec<AssignmentRecord>,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    tenant_id: Uuid,
    email: String,
    display_name: String,
    job_title: Option<String>,
    status: String,
    locale: String,
    clearance: String,
    attributes: Value,
}

#[derive(sqlx::FromRow)]
struct AssignmentRow {
    role_id: Uuid,
    scope_type: String,
    scope_org_unit_id: Option<Uuid>,
    include_descendants: bool,
    expires_at: Option<DateTime<Utc>>,
    role_name: String,
}

pub(crate) fn to_scope(
    scope_type: &str,
    org_unit_id: Option<Uuid>,
    include_descendants: bool,
) -> AssignmentScope {
    match (scope_type, org_unit_id) {
        ("org_unit", Some(org_unit_id)) => AssignmentScope::OrgUnit {
            org_unit_id,
            include_descendants,
        },
        ("tenant", _) => AssignmentScope::Tenant,
        _ => AssignmentScope::Own,
    }
}

/// A user with memberships and role assignments, ready for the engine.
pub async fn load_user_access(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> anyhow::Result<Option<UserAccess>> {
    let Some(user): Option<UserRow> = sqlx::query_as(
        "select id, tenant_id, email::text as email, display_name, job_title, status, locale, \
         clearance, attributes from users where id = $1",
    )
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let units: Vec<Uuid> = sqlx::query_scalar(&format!(
        "select org_unit_id from org_memberships where user_id = $1 order by {PRIMARY_UNIT_FIRST}"
    ))
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    let rows: Vec<AssignmentRow> = sqlx::query_as(
        "select a.role_id, a.scope_type, a.scope_org_unit_id, a.include_descendants, a.expires_at, \
         r.name as role_name \
         from role_assignments a join roles r on r.id = a.role_id where a.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;

    let assignments: Vec<AssignmentRecord> = rows
        .into_iter()
        .map(|a| AssignmentRecord {
            assignment: RoleAssignment {
                role_id: a.role_id,
                scope: to_scope(&a.scope_type, a.scope_org_unit_id, a.include_descendants),
                expires_at: a.expires_at,
            },
            role_name: a.role_name,
        })
        .collect();
    let clearance: Sensitivity = user.clearance.parse().map_err(|e: String| anyhow!(e))?;
    Ok(Some(UserAccess {
        profile: UserProfile {
            id: user.id,
            tenant_id: user.tenant_id,
            email: user.email,
            display_name: user.display_name,
            job_title: user.job_title,
            status: user.status,
            locale: user.locale,
            primary_unit_id: units.first().copied(),
        },
        subject: Subject {
            tenant_id: user.tenant_id,
            user_id: user.id,
            org_unit_ids: units,
            assignments: assignments.iter().map(|a| a.assignment.clone()).collect(),
            clearance: Some(clearance),
            attributes: match user.attributes {
                Value::Object(map) => map,
                _ => Map::new(),
            },
        },
        assignments,
    }))
}
