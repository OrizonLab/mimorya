//! Accounts: creation with a temporary password, profile and status changes,
//! password reset. A disabled account loses its sessions at once.

use mimorya_core::DomainError;
use mimorya_core::authz::AssignmentScope;
use mimorya_core::authz::Sensitivity;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::role_assignments::{
    NewRoleAssignment, create_role_assignment, find_role_id,
};
use mimorya_db::repositories::users::{
    NewUser, UserPatch, create_user as insert_user, get_user, revoke_user_sessions,
    set_password_hash, set_primary_unit, update_user as patch_user,
};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, is_unique_violation, refuse_self};
use crate::auth::credentials::{generate_temporary_password, hash_password_async};
use crate::services::AccessContext;

/// Base role every account receives on its own data (agent, directory…).
const BASE_ROLE_KEY: &str = "member";

pub struct NewUserInput {
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub org_unit_id: Option<Uuid>,
    pub clearance: Sensitivity,
}

/// Returns the new account and its temporary password (shown once).
pub async fn create_user(
    db: &Database,
    access: &AccessContext,
    input: NewUserInput,
) -> anyhow::Result<(Uuid, String)> {
    access.require_tenant_wide("user:manage")?;
    check_unit(access, input.org_unit_id)?;
    let password = generate_temporary_password();
    let password_hash = hash_password_async(password.clone()).await?;
    let tenant_id = access.subject.tenant_id;

    let mut tx = db.begin(tenant_id).await?;
    let id = match insert_user(
        &mut tx,
        NewUser {
            tenant_id,
            email: input.email.to_lowercase(),
            display_name: input.display_name,
            job_title: input.job_title,
            clearance: input.clearance,
            password_hash,
        },
    )
    .await
    {
        Ok(id) => id,
        // Addresses are unique across companies (one account, one company).
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict("Cette adresse e-mail est déjà utilisée").into());
        }
        Err(error) => return Err(error.into()),
    };
    set_primary_unit(&mut tx, tenant_id, id, input.org_unit_id).await?;
    grant_base_role(&mut tx, access, id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "user.created", "user", id)
            .metadata(json!({ "clearance": input.clearance.as_str() })),
    )
    .await?;
    tx.commit().await?;
    Ok((id, password))
}

async fn grant_base_role(
    tx: &mut TenantTx,
    access: &AccessContext,
    user_id: Uuid,
) -> anyhow::Result<()> {
    let Some(role_id) = find_role_id(tx, BASE_ROLE_KEY).await? else {
        return Ok(());
    };
    create_role_assignment(
        tx,
        NewRoleAssignment {
            tenant_id: access.subject.tenant_id,
            user_id,
            role_id,
            scope: AssignmentScope::Own,
            granted_by_user_id: Some(access.subject.user_id),
            expires_at: None,
        },
    )
    .await?;
    Ok(())
}

/// Fields left to `None` are unchanged.
#[derive(Default)]
pub struct UserChanges {
    pub display_name: Option<String>,
    pub job_title: Option<Option<String>>,
    pub org_unit_id: Option<Option<Uuid>>,
    pub clearance: Option<Sensitivity>,
    /// `true` to enable, `false` to disable the account.
    pub active: Option<bool>,
}

pub async fn update_user(
    db: &Database,
    access: &AccessContext,
    user_id: Uuid,
    changes: UserChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide("user:manage")?;
    refuse_self(access, user_id)?;
    if let Some(unit) = changes.org_unit_id {
        check_unit(access, unit)?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = db.begin(tenant_id).await?;
    let status = changes
        .active
        .map(|a| if a { "active" } else { "disabled" });
    let found = patch_user(
        &mut tx,
        user_id,
        UserPatch {
            display_name: changes.display_name,
            job_title: changes.job_title,
            clearance: changes.clearance,
            status,
        },
    )
    .await?;
    if !found {
        return Err(DomainError::not_found("Compte introuvable").into());
    }
    if let Some(unit) = changes.org_unit_id {
        set_primary_unit(&mut tx, tenant_id, user_id, unit).await?;
    }
    if changes.active == Some(false) {
        revoke_user_sessions(&mut tx, user_id, None).await?;
    }
    let action = match changes.active {
        Some(true) => "user.enabled",
        Some(false) => "user.disabled",
        None => "user.updated",
    };
    let metadata = json!({
        "clearance": changes.clearance.map(Sensitivity::as_str),
        "orgUnitChanged": changes.org_unit_id.is_some(),
    });
    append_audit(
        &mut tx,
        audit_entry(access, action, "user", user_id).metadata(metadata),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Replaces the password with a temporary one (shown once) and ends the
/// account's sessions.
pub async fn reset_password(
    db: &Database,
    access: &AccessContext,
    user_id: Uuid,
) -> anyhow::Result<String> {
    access.require_tenant_wide("user:manage")?;
    refuse_self(access, user_id)?;
    let password = generate_temporary_password();
    let hash = hash_password_async(password.clone()).await?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    if get_user(&mut tx, user_id).await?.is_none() {
        return Err(DomainError::not_found("Compte introuvable").into());
    }
    set_password_hash(&mut tx, user_id, &hash).await?;
    revoke_user_sessions(&mut tx, user_id, None).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "user.password_reset", "user", user_id),
    )
    .await?;
    tx.commit().await?;
    Ok(password)
}

fn check_unit(access: &AccessContext, unit: Option<Uuid>) -> Result<(), DomainError> {
    match unit {
        Some(id) if access.org_tree().get(id).is_none() => {
            Err(DomainError::validation("Unité inconnue"))
        }
        _ => Ok(()),
    }
}
