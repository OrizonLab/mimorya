//! Accounts: creation (invited, ADR-027), profile, attributes (read by access
//! policies) and status changes, password reset to a temporary password. A
//! disabled account loses its sessions and its pending invitation at once.

use std::collections::BTreeSet;

use mimorya_core::DomainError;
use mimorya_core::authz::{AssignmentScope, OrgTree, Sensitivity};
use mimorya_core::collaborator_attributes::normalize_attributes;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::invitations::delete_invitation_of;
use mimorya_db::repositories::role_assignments::{
    NewRoleAssignment, create_role_assignment, find_role_id,
};
use mimorya_db::repositories::users::{
    NewUser, UserPatch, create_user as insert_user, get_user, revoke_user_sessions,
    set_primary_unit, set_temporary_password_hash, update_user as patch_user,
};
use mimorya_db::{Database, TenantTx, is_unique_violation};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::{audit_entry, begin_change, current_tree, refuse_self};
use crate::auth::credentials::{generate_temporary_password, hash_password_async};
use crate::services::AccessContext;

/// Base role every account receives on its own data (agent, directory…).
const BASE_ROLE_KEY: &str = "member";

const PERMISSION: &str = "user:manage";

pub struct NewUserInput {
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub org_unit_id: Option<Uuid>,
    pub clearance: Sensitivity,
}

/// Creates an `invited` account, without password: it chooses one through
/// its invitation link (`issue_invitation`, ADR-027).
pub async fn create_user(
    db: &Database,
    access: &AccessContext,
    input: NewUserInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;

    let mut tx = begin_change(db, access, PERMISSION).await?;
    check_unit(&current_tree(&mut tx).await?, input.org_unit_id)?;
    let id = match insert_user(
        &mut tx,
        NewUser {
            tenant_id,
            email: input.email.to_lowercase(),
            display_name: input.display_name,
            job_title: input.job_title,
            clearance: input.clearance,
            password_hash: None,
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
    Ok(id)
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
    /// Replaces every attribute (`subject.attributes.*` in policy conditions).
    pub attributes: Option<Map<String, Value>>,
}

pub async fn update_user(
    db: &Database,
    access: &AccessContext,
    user_id: Uuid,
    changes: UserChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    let attributes = changes
        .attributes
        .as_ref()
        .map(normalize_attributes)
        .transpose()
        .map_err(DomainError::validation)?
        .map(Value::Object);
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if let Some(unit) = changes.org_unit_id {
        check_unit(&current_tree(&mut tx).await?, unit)?;
    }
    // Who a policy applies to may change: the audit names the attributes
    // that changed, never their values (personal data, kept as long as the log).
    let changed_attributes = match &attributes {
        Some(next) => {
            let previous = get_user(&mut tx, user_id).await?.map(|u| u.attributes);
            Some(changed_names(previous.as_ref(), next))
        }
        None => None,
    };
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
            attributes: attributes.clone(),
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
        delete_invitation_of(&mut tx, user_id).await?;
    }
    let action = match changes.active {
        Some(true) => "user.enabled",
        Some(false) => "user.disabled",
        None => "user.updated",
    };
    let metadata = json!({
        "clearance": changes.clearance.map(Sensitivity::as_str),
        "orgUnitChanged": changes.org_unit_id.is_some(),
        "attributesChanged": changed_attributes,
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
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    let password = generate_temporary_password();
    let hash = hash_password_async(password.clone()).await?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let Some(user) = get_user(&mut tx, user_id).await? else {
        return Err(DomainError::not_found("Compte introuvable").into());
    };
    if user.status == "invited" {
        return Err(DomainError::conflict(
            "Ce compte n’a pas encore choisi son mot de passe : renvoyez-lui plutôt son invitation",
        )
        .into());
    }
    set_temporary_password_hash(&mut tx, user_id, &hash).await?;
    revoke_user_sessions(&mut tx, user_id, None).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "user.password_reset", "user", user_id),
    )
    .await?;
    tx.commit().await?;
    Ok(password)
}

/// The attributes added, removed or given another value, sorted.
fn changed_names(previous: Option<&Value>, next: &Value) -> Vec<String> {
    let empty = Map::new();
    let previous = previous.and_then(Value::as_object).unwrap_or(&empty);
    let next = next.as_object().unwrap_or(&empty);
    let names: BTreeSet<&String> = previous
        .keys()
        .chain(next.keys())
        .filter(|name| previous.get(*name) != next.get(*name))
        .collect();
    names.into_iter().cloned().collect()
}

fn check_unit(tree: &OrgTree, unit: Option<Uuid>) -> Result<(), DomainError> {
    match unit {
        Some(id) if tree.get(id).is_none() => {
            Err(DomainError::unknown("Unité inconnue", "org_unit", id))
        }
        _ => Ok(()),
    }
}
