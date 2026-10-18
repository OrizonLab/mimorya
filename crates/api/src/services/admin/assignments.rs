//! Granting and revoking roles. Administration roles are only granted on the
//! whole company; nobody changes their own rights; the same role is not held
//! twice on the same scope.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_core::administration::assignment_problem;
use mimorya_core::authz::AssignmentScope;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::role_assignments::{
    NewRoleAssignment, create_role_assignment, delete_role_assignment, get_role_assignment,
    has_role_assignment,
};
use mimorya_db::repositories::roles::get_role;
use mimorya_db::repositories::users::get_user;
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change, current_tree, refuse_self};
use crate::services::AccessContext;

const PERMISSION: &str = "role:manage";

pub async fn grant_role(
    db: &Database,
    access: &AccessContext,
    user_id: Uuid,
    role_id: Uuid,
    scope: AssignmentScope,
    expires_at: Option<DateTime<Utc>>,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    if expires_at.is_some_and(|at| at <= Utc::now()) {
        return Err(DomainError::validation("La date d’échéance doit être dans le futur").into());
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if get_user(&mut tx, user_id).await?.is_none() {
        return Err(DomainError::not_found("Compte introuvable").into());
    }
    // As stored now: an administrator may have just changed its permissions.
    let Some(role) = get_role(&mut tx, role_id).await? else {
        return Err(DomainError::validation("Rôle inconnu").into());
    };
    let tree = current_tree(&mut tx).await?;
    if let Some(problem) = assignment_problem(&role.permissions, &scope, &tree) {
        return Err(DomainError::validation(problem).into());
    }
    if has_role_assignment(&mut tx, user_id, role_id, &scope).await? {
        return Err(DomainError::conflict("Ce rôle est déjà attribué sur ce périmètre").into());
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
            "role": role.key,
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
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
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
