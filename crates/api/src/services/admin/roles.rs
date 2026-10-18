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
    role_name_taken, update_custom_role,
};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change, is_unique_violation};
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
    let Some(key) = custom_role_key(&input.name) else {
        return Err(DomainError::validation(
            "Le nom doit contenir au moins une lettre ou un chiffre",
        )
        .into());
    };
    let permissions =
        normalize_role_permissions(&input.permissions, catalog).map_err(DomainError::validation)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if role_name_taken(&mut tx, &input.name, None).await? {
        return Err(DomainError::conflict(NAME_TAKEN).into());
    }
    let created = insert_role(
        &mut tx,
        NewRole {
            tenant_id,
            key: key.clone(),
            name: input.name,
            description: input.description,
            permissions: permissions.clone(),
        },
    )
    .await;
    let id = match created {
        Ok(id) => id,
        // Another name giving the same key ("Chef d’équipe" / "Chef d'équipe").
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict(NAME_TAKEN).into());
        }
        Err(error) => return Err(error.into()),
    };
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
    let metadata = json!({ "key": role.key, "permissions": permissions });
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
