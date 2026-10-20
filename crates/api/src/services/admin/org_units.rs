//! Organisation tree changes: add, rename, retype, move, change the lead,
//! delete an unused unit. The rules are checked on the tree as stored, under
//! the administration lock; the cached tree of the tenant is refreshed after
//! each change.

use mimorya_core::DomainError;
use mimorya_core::administration::{move_problem, unit_type_problem};
use mimorya_core::authz::OrgUnitType;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::org_units::{
    NewOrgUnit, OrgUnitPatch, create_org_unit, delete_org_unit, lock_org_unit, org_unit_usage,
    update_org_unit,
};
use mimorya_db::repositories::users::get_user;
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change, current_tree};
use crate::services::{AccessContext, AccessService};

const PERMISSION: &str = "org:manage";

pub struct NewUnitInput {
    pub parent_id: Uuid,
    pub name: String,
    pub unit_type: OrgUnitType,
}

pub async fn create_unit(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    input: NewUnitInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    if let Some(problem) = unit_type_problem(false, input.unit_type) {
        return Err(DomainError::validation(problem).into());
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if current_tree(&mut tx).await?.get(input.parent_id).is_none() {
        let parent = input.parent_id;
        return Err(DomainError::unknown("Unité parente inconnue", "org_unit", parent).into());
    }
    let id = create_org_unit(
        &mut tx,
        NewOrgUnit {
            tenant_id,
            parent_id: input.parent_id,
            name: input.name,
            unit_type: input.unit_type.as_str(),
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "org_unit.created", "org_unit", id)
            .metadata(json!({ "parentId": input.parent_id })),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(tenant_id);
    Ok(id)
}

/// Fields left to `None` are unchanged.
#[derive(Default)]
pub struct UnitChanges {
    pub name: Option<String>,
    pub unit_type: Option<OrgUnitType>,
    pub parent_id: Option<Uuid>,
    pub lead_user_id: Option<Option<Uuid>>,
}

pub async fn update_unit(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    id: Uuid,
    changes: UnitChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let tree = current_tree(&mut tx).await?;
    let Some(unit) = tree.get(id) else {
        return Err(DomainError::not_found("Unité introuvable").into());
    };
    if let Some(unit_type) = changes.unit_type
        && let Some(problem) = unit_type_problem(unit.parent_id.is_none(), unit_type)
    {
        return Err(DomainError::validation(problem).into());
    }
    if let Some(parent) = changes.parent_id {
        if tree.get(parent).is_none() {
            return Err(DomainError::unknown("Unité parente inconnue", "org_unit", parent).into());
        }
        if let Some(problem) = move_problem(&tree, id, parent) {
            return Err(DomainError::conflict(problem).into());
        }
    }
    if let Some(Some(lead)) = changes.lead_user_id
        && get_user(&mut tx, lead).await?.is_none()
    {
        return Err(DomainError::unknown("Responsable inconnu", "user", lead).into());
    }
    let metadata = json!({
        "parentId": changes.parent_id,
        "leadChanged": changes.lead_user_id.is_some(),
    });
    update_org_unit(
        &mut tx,
        id,
        OrgUnitPatch {
            name: changes.name,
            unit_type: changes.unit_type.map(OrgUnitType::as_str),
            parent_id: changes.parent_id,
            lead_user_id: changes.lead_user_id,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "org_unit.updated", "org_unit", id).metadata(metadata),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(tenant_id);
    Ok(())
}

/// Only an unused unit can be deleted: deleting a used one would silently
/// drop memberships, role assignments and team memory (cascades). The unit's
/// row is locked before counting, so nothing new can reference it meanwhile.
pub async fn delete_unit(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let tree = current_tree(&mut tx).await?;
    let Some(unit) = tree.get(id) else {
        return Err(DomainError::not_found("Unité introuvable").into());
    };
    if unit.parent_id.is_none() {
        return Err(DomainError::conflict("L’unité racine ne peut pas être supprimée").into());
    }
    lock_org_unit(&mut tx, id).await?;
    let usage = org_unit_usage(&mut tx, id).await?;
    if !usage.is_unused() {
        let uses: Vec<String> = [
            (usage.children, "sous-unité(s)"),
            (usage.members, "membre(s)"),
            (usage.assignments, "habilitation(s)"),
            (usage.memories, "élément(s) de mémoire"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, what)| format!("{count} {what}"))
        .collect();
        return Err(
            DomainError::conflict(format!("Unité encore utilisée : {}", uses.join(", "))).into(),
        );
    }
    delete_org_unit(&mut tx, id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "org_unit.deleted", "org_unit", id)
            .metadata(json!({ "name": unit.name })),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(tenant_id);
    Ok(())
}
