//! Demo tenant, accounts and access model: users, org units, memberships,
//! roles, scoped assignments and ABAC policies (FICTIONAL, core `demo`).

use mimorya_core::authz::AssignmentScope;
use mimorya_core::demo::{self, tenant, user};
use serde_json::{Value, json};
use sqlx::PgConnection;

pub(super) async fn insert(
    conn: &mut PgConnection,
    password_hash: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query("insert into tenants (id, slug, name, settings) values ($1, $2, $3, $4)")
        .bind(tenant::ID)
        .bind(tenant::SLUG)
        .bind(tenant::NAME)
        .bind(json!({ "demo": true, "defaultAutonomyLevel": "execute_with_approval" }))
        .execute(&mut *conn)
        .await?;

    let users = demo::users();
    for u in &users {
        sqlx::query(
            "insert into users (id, tenant_id, email, display_name, job_title, password_hash, clearance, attributes) \
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(u.id)
        .bind(tenant::ID)
        .bind(&u.email)
        .bind(&u.display_name)
        .bind(&u.job_title)
        .bind(password_hash)
        .bind(u.clearance.as_str())
        .bind(Value::Object(u.attributes.clone()))
        .execute(&mut *conn)
        .await?;
    }

    // Parents are listed before children in the fixture.
    for unit in demo::org_units() {
        sqlx::query(
            "insert into org_units (id, tenant_id, parent_id, name, type, lead_user_id) values ($1, $2, $3, $4, $5, $6)",
        )
        .bind(unit.id)
        .bind(tenant::ID)
        .bind(unit.parent_id)
        .bind(&unit.name)
        .bind(unit.unit_type.as_str())
        .bind(unit.lead_user_id)
        .execute(&mut *conn)
        .await?;
    }

    for u in &users {
        sqlx::query(
            "insert into org_memberships (tenant_id, user_id, org_unit_id, is_primary) values ($1, $2, $3, true)",
        )
        .bind(tenant::ID)
        .bind(u.id)
        .bind(u.primary_unit_id)
        .execute(&mut *conn)
        .await?;
    }

    for role in demo::role_definitions() {
        sqlx::query(
            "insert into roles (id, tenant_id, key, name, description, permissions, is_system) \
             values ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(role.id)
        .bind(tenant::ID)
        .bind(&role.key)
        .bind(&role.name)
        .bind(&role.description)
        .bind(&role.permissions)
        .bind(role.is_system)
        .execute(&mut *conn)
        .await?;
    }

    for u in &users {
        for assignment in &u.assignments {
            let (scope_type, unit, descendants) = match assignment.scope {
                AssignmentScope::Tenant => ("tenant", None, true),
                AssignmentScope::Own => ("self", None, true),
                AssignmentScope::OrgUnit {
                    org_unit_id,
                    include_descendants,
                } => ("org_unit", Some(org_unit_id), include_descendants),
            };
            sqlx::query(
                "insert into role_assignments (tenant_id, user_id, role_id, scope_type, scope_org_unit_id, \
                 include_descendants, granted_by_user_id) values ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(tenant::ID)
            .bind(u.id)
            .bind(assignment.role_id)
            .bind(scope_type)
            .bind(unit)
            .bind(descendants)
            .bind(user::PAUL)
            .execute(&mut *conn)
            .await?;
        }
    }

    for policy in demo::policies() {
        sqlx::query(
            "insert into access_policies (id, tenant_id, description, effect, permissions, condition) \
             values ($1, $2, $3, $4, $5, $6)",
        )
        .bind(policy.id)
        .bind(tenant::ID)
        .bind(&policy.description)
        .bind(match policy.effect {
            mimorya_core::authz::PolicyEffect::Allow => "allow",
            mimorya_core::authz::PolicyEffect::Deny => "deny",
        })
        .bind(&policy.permissions)
        .bind(serde_json::to_value(&policy.condition)?)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
