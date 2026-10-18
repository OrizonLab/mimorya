//! Custom roles: creation, change and deletion by an administrator. Platform
//! roles are fixed; nobody changes a role they hold (it would change their own
//! rights); a role granting administration permissions stays company-wide; a
//! role still assigned is not deleted (the cascade would silently take rights
//! away). Holders' rights follow a change at once (cache refreshed).

use mimorya_core::DomainError;
use mimorya_core::administration::requires_tenant_scope;
use mimorya_core::custom_roles::{custom_role_key, normalize_role_permissions};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::role_assignments::holds_role;
use mimorya_db::repositories::roles::{
    NewRole, RolePatch, RoleRow, create_role as insert_role, delete_custom_role, get_role,
    role_key_taken, role_name_taken, update_custom_role,
};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change};
use crate::services::{AccessContext, AccessService};

const PERMISSION: &str = "role:manage";
const NAME_TAKEN: &str = "Un rôle porte déjà ce nom";

pub struct RoleInput {
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
}

pub async fn create_role(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    catalog: &[&str],
    input: RoleInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let base_key = key_of(&input.name)?;
    let permissions =
        normalize_role_permissions(&input.permissions, catalog).map_err(DomainError::validation)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if role_name_taken(&mut tx, &input.name, None).await? {
        return Err(DomainError::conflict(NAME_TAKEN).into());
    }
    let key = free_key(&mut tx, &base_key).await?;
    let id = insert_role(
        &mut tx,
        NewRole {
            tenant_id,
            key: key.clone(),
            name: input.name,
            description: input.description,
            permissions: permissions.clone(),
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "role.created", "role", id)
            .metadata(json!({ "key": key, "permissions": permissions })),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(tenant_id);
    Ok(id)
}

/// Fields left to `None` are unchanged.
#[derive(Default)]
pub struct RoleChanges {
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
}

pub async fn update_role(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    catalog: &[&str],
    id: Uuid,
    changes: RoleChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    if let Some(name) = &changes.name {
        key_of(name)?;
    }
    let permissions = changes
        .permissions
        .map(|p| normalize_role_permissions(&p, catalog))
        .transpose()
        .map_err(DomainError::validation)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let role = editable_role(&mut tx, access, id).await?;
    if let Some(name) = &changes.name
        && role_name_taken(&mut tx, name, Some(id)).await?
    {
        return Err(DomainError::conflict(NAME_TAKEN).into());
    }
    if let Some(permissions) = &permissions
        && requires_tenant_scope(permissions)
        && role.scoped_assignments > 0
    {
        return Err(DomainError::conflict(format!(
            "Un rôle d’administration ne s’attribue que sur toute l’entreprise : retirez d’abord \
             ses {} habilitation(s) limitée(s) à une unité ou à ses propres données",
            role.scoped_assignments
        ))
        .into());
    }
    let metadata = json!({
        "key": role.key,
        "name": changes.name,
        "previousName": changes.name.as_ref().map(|_| &role.name),
        "descriptionChanged": changes.description.is_some(),
        "permissions": permissions,
        "previousPermissions": permissions.as_ref().map(|_| &role.permissions),
    });
    update_custom_role(
        &mut tx,
        id,
        RolePatch {
            name: changes.name,
            description: changes.description,
            permissions,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "role.updated", "role", id).metadata(metadata),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(access.subject.tenant_id);
    Ok(())
}

pub async fn delete_role(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let role = editable_role(&mut tx, access, id).await?;
    if role.assignments > 0 {
        return Err(DomainError::conflict(format!(
            "Rôle encore attribué : {} habilitation(s) à retirer d’abord",
            role.assignments
        ))
        .into());
    }
    delete_custom_role(&mut tx, id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "role.deleted", "role", id)
            .metadata(json!({ "key": role.key, "name": role.name })),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(access.subject.tenant_id);
    Ok(())
}

/// The key a name gives, or why the name is refused.
fn key_of(name: &str) -> Result<String, DomainError> {
    custom_role_key(name).ok_or_else(|| {
        DomainError::validation("Le nom doit contenir au moins une lettre ou un chiffre")
    })
}

/// The first free key from `base` (`base`, `base_2`, `base_3`…): a renamed
/// role keeps its key, which its former name would otherwise give again.
/// Read under the administration lock, so no other creation takes it meanwhile.
async fn free_key(tx: &mut TenantTx, base: &str) -> anyhow::Result<String> {
    for n in 1..=1000 {
        let key = if n == 1 {
            base.to_string()
        } else {
            format!("{base}_{n}")
        };
        if !role_key_taken(tx, &key).await? {
            return Ok(key);
        }
    }
    Err(DomainError::conflict("Trop de rôles portent un nom proche").into())
}

/// The custom role, as stored now, that the administrator may change.
async fn editable_role(
    tx: &mut TenantTx,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<RoleRow> {
    let Some(role) = get_role(tx, id).await? else {
        return Err(DomainError::not_found("Rôle introuvable").into());
    };
    if role.is_system {
        return Err(
            DomainError::forbidden("Les rôles de la plateforme ne se modifient pas").into(),
        );
    }
    if holds_role(tx, access.subject.user_id, id).await? {
        return Err(DomainError::forbidden(
            "Vous ne pouvez pas modifier un rôle que vous détenez : il faut un autre administrateur",
        )
        .into());
    }
    Ok(role)
}
