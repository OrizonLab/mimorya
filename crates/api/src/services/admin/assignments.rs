//! Granting and revoking roles. Administration roles are only granted on the
//! whole company; nobody changes their own rights.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_core::administration::assignment_problem;
use mimorya_core::authz::AssignmentScope;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::role_assignments::{
    NewRoleAssignment, create_role_assignment, delete_role_assignment, get_role_assignment,
};
use mimorya_db::repositories::users::get_user;
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, refuse_self};
use crate::services::AccessContext;

pub async fn grant_role(
    db: &Database,
    access: &AccessContext,
    user_id: Uuid,
    role_id: Uuid,
    scope: AssignmentScope,
    expires_at: Option<DateTime<Utc>>,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide("role:manage")?;
    refuse_self(access, user_id)?;
    let Some(role) = access.roles.iter().find(|r| r.definition.id == role_id) else {
        return Err(DomainError::validation("Rôle inconnu").into());
    };
    if let Some(problem) =
        assignment_problem(&role.definition.permissions, &scope, access.org_tree())
    {
        return Err(DomainError::validation(problem).into());
    }
    if expires_at.is_some_and(|at| at <= Utc::now()) {
        return Err(DomainError::validation("La date d’échéance doit être dans le futur").into());
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = db.begin(tenant_id).await?;
    if get_user(&mut tx, user_id).await?.is_none() {
        return Err(DomainError::not_found("Compte introuvable").into());
    }
    let id = create_role_assignment(
        &mut tx,
        NewRoleAssignment {
            tenant_id,
            user_id,
            role_id,
            scope: scope.clone(),
            granted_by_user_id: Some(access.subject.user_id),
            expires_at,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "role.assigned", "user", user_id).metadata(json!({
            "assignmentId": id,
            "role": role.definition.key,
            "scope": scope,
            "expiresAt": expires_at,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn revoke_role(
    db: &Database,
    access: &AccessContext,
    assignment_id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide("role:manage")?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let Some(assignment) = get_role_assignment(&mut tx, assignment_id).await? else {
        return Err(DomainError::not_found("Habilitation introuvable").into());
    };
    refuse_self(access, assignment.user_id)?;
    delete_role_assignment(&mut tx, assignment_id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "role.revoked", "user", assignment.user_id).metadata(json!({
            "assignmentId": assignment_id,
            "role": assignment.role_name,
            "scope": assignment.scope(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
