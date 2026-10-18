//! Role assignments: who holds which role, on which scope, until when.

use chrono::{DateTime, Utc};
use mimorya_core::authz::AssignmentScope;
use sqlx::PgConnection;
use uuid::Uuid;

use super::org::to_scope;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoleAssignmentRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub role_name: String,
    pub scope_type: String,
    pub scope_org_unit_id: Option<Uuid>,
    pub include_descendants: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

impl RoleAssignmentRow {
    pub fn scope(&self) -> AssignmentScope {
        to_scope(
            &self.scope_type,
            self.scope_org_unit_id,
            self.include_descendants,
        )
    }
}

const ASSIGNMENT_SELECT: &str = "select a.id, a.user_id, a.role_id, r.name as role_name, \
    a.scope_type, a.scope_org_unit_id, a.include_descendants, a.expires_at \
    from role_assignments a join roles r on r.id = a.role_id";

pub async fn list_role_assignments(
    conn: &mut PgConnection,
) -> sqlx::Result<Vec<RoleAssignmentRow>> {
    sqlx::query_as(&format!("{ASSIGNMENT_SELECT} order by a.created_at"))
        .fetch_all(conn)
        .await
}

pub async fn get_role_assignment(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<RoleAssignmentRow>> {
    sqlx::query_as(&format!("{ASSIGNMENT_SELECT} where a.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

#[derive(Debug, Clone)]
pub struct NewRoleAssignment {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub scope: AssignmentScope,
    pub granted_by_user_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Stored form of a scope: `scope_type`, `scope_org_unit_id`, `include_descendants`.
fn scope_columns(scope: &AssignmentScope) -> (&'static str, Option<Uuid>, bool) {
    match *scope {
        AssignmentScope::Tenant => ("tenant", None, true),
        AssignmentScope::OrgUnit {
            org_unit_id,
            include_descendants,
        } => ("org_unit", Some(org_unit_id), include_descendants),
        AssignmentScope::Own => ("self", None, true),
    }
}

/// Whether the user already holds the role on exactly this scope (an expired
/// assignment does not count).
pub async fn has_role_assignment(
    conn: &mut PgConnection,
    user_id: Uuid,
    role_id: Uuid,
    scope: &AssignmentScope,
) -> sqlx::Result<bool> {
    let (scope_type, unit, descendants) = scope_columns(scope);
    sqlx::query_scalar(
        "select exists (select 1 from role_assignments where user_id = $1 and role_id = $2 \
         and scope_type = $3 and scope_org_unit_id is not distinct from $4 \
         and include_descendants = $5 and (expires_at is null or expires_at > now()))",
    )
    .bind(user_id)
    .bind(role_id)
    .bind(scope_type)
    .bind(unit)
    .bind(descendants)
    .fetch_one(conn)
    .await
}

pub async fn create_role_assignment(
    conn: &mut PgConnection,
    input: NewRoleAssignment,
) -> sqlx::Result<Uuid> {
    let (scope_type, unit, descendants) = scope_columns(&input.scope);
    sqlx::query_scalar(
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type, scope_org_unit_id, \
         include_descendants, granted_by_user_id, expires_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8) returning id",
    )
    .bind(input.tenant_id)
    .bind(input.user_id)
    .bind(input.role_id)
    .bind(scope_type)
    .bind(unit)
    .bind(descendants)
    .bind(input.granted_by_user_id)
    .bind(input.expires_at)
    .fetch_one(conn)
    .await
}

pub async fn delete_role_assignment(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from role_assignments where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Identifier of a role of the tenant by its key (e.g. `member`).
pub async fn find_role_id(conn: &mut PgConnection, key: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("select id from roles where key = $1")
        .bind(key)
        .fetch_optional(conn)
        .await
}
