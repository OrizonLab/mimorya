//! Access policies (ADR-022): an administrator creates, changes, disables and
//! deletes deny policies on business permissions, at once. An allow policy is
//! written only once another administrator approved its proposal
//! (`policy_proposals`, ADR-168); here it is only switched off or deleted,
//! which only takes access away. Conditions are checked before they are
//! stored (`policy_checks`). Changes apply at once (cache refreshed).

use mimorya_core::DomainError;
use mimorya_core::authz::{Condition, PermissionInfo};
use mimorya_core::policy_rules::normalize_policy_permissions;
use mimorya_db::repositories::access_policies::{
    NewPolicy, PolicyPatch, PolicyRow, create_deny_policy, delete_allow_policy, delete_deny_policy,
    get_policy, switch_off_allow_policy, update_deny_policy,
};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::policy_checks::{
    checked_condition, checked_permissions, checked_scope, stored_condition,
};
use super::{audit_entry, begin_change};
use crate::services::{AccessContext, AccessService};

const PERMISSION: &str = "policy:manage";

pub struct PolicyInput {
    pub description: String,
    pub permissions: Vec<String>,
    pub condition: Condition,
    pub enabled: bool,
}

pub async fn create_policy(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    catalog: &[PermissionInfo],
    input: PolicyInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let permissions = checked_permissions(&input.permissions, catalog)?;
    let condition = checked_condition(&input.condition)?;
    checked_scope(&permissions, &input.condition, catalog)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let id = create_deny_policy(
        &mut tx,
        NewPolicy {
            tenant_id,
            description: input.description.clone(),
            permissions: permissions.clone(),
            condition: condition.clone(),
            enabled: input.enabled,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "policy.created", "policy", id).metadata(json!({
            "description": input.description,
            "permissions": permissions,
            "condition": condition,
            "enabled": input.enabled,
        })),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(tenant_id);
    Ok(id)
}

/// Fields left to `None` are unchanged.
#[derive(Default)]
pub struct PolicyChanges {
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub condition: Option<Condition>,
    pub enabled: Option<bool>,
}

pub async fn update_policy(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    catalog: &[PermissionInfo],
    id: Uuid,
    changes: PolicyChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let only_switches_off = matches!(
        changes,
        PolicyChanges {
            description: None,
            permissions: None,
            condition: None,
            enabled: Some(false),
        }
    );
    let permissions = changes
        .permissions
        .map(|p| checked_permissions(&p, catalog))
        .transpose()?;
    let condition = changes
        .condition
        .as_ref()
        .map(checked_condition)
        .transpose()?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let previous = existing_policy(&mut tx, id).await?;
    if previous.effect != "deny" {
        return switch_off(tx, access_service, access, previous, only_switches_off).await;
    }
    // The policy as it will stand is checked whenever it acts or its rule
    // changes: what is stored may predate a rule or have been written by
    // hand, and enabling must never load what the API would refuse. A
    // disabled policy can still be renamed, or switched off, whatever it holds.
    let acts = changes.enabled.unwrap_or(previous.enabled);
    if acts || permissions.is_some() || changes.condition.is_some() {
        let effective_permissions = match &permissions {
            Some(permissions) => permissions.clone(),
            None => stored_permissions(&previous, catalog)?,
        };
        let effective_condition = match changes.condition.clone() {
            Some(condition) => condition,
            None => stored_condition(&previous.condition)?,
        };
        checked_scope(&effective_permissions, &effective_condition, catalog)?;
    }
    let metadata = json!({
        "description": changes.description,
        "previousDescription": changes.description.as_ref().map(|_| &previous.description),
        "permissions": permissions,
        "previousPermissions": permissions.as_ref().map(|_| &previous.permissions),
        "condition": condition,
        "previousCondition": condition.as_ref().map(|_| &previous.condition),
        "enabled": changes.enabled,
        "previousEnabled": changes.enabled.map(|_| previous.enabled),
    });
    update_deny_policy(
        &mut tx,
        id,
        PolicyPatch {
            description: changes.description,
            permissions,
            condition,
            enabled: changes.enabled,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "policy.updated", "policy", id).metadata(metadata),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(access.subject.tenant_id);
    Ok(())
}

pub async fn delete_policy(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let previous = existing_policy(&mut tx, id).await?;
    // Deleting an allow policy only takes access away (ADR-168).
    if previous.effect == "deny" {
        delete_deny_policy(&mut tx, id).await?;
    } else {
        delete_allow_policy(&mut tx, id).await?;
    }
    append_audit(
        &mut tx,
        audit_entry(access, "policy.deleted", "policy", id).metadata(json!({
            "effect": previous.effect,
            "description": previous.description,
            "permissions": previous.permissions,
            "condition": previous.condition,
        })),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(access.subject.tenant_id);
    Ok(())
}

/// The stored permissions, checked again (they may predate a rule).
fn stored_permissions(
    policy: &PolicyRow,
    catalog: &[PermissionInfo],
) -> Result<Vec<String>, DomainError> {
    normalize_policy_permissions(&policy.permissions, catalog).map_err(|problem| {
        DomainError::validation(format!(
            "Les permissions enregistrées ne sont plus acceptées ({problem}) : modifiez-les \
             ou désactivez la politique"
        ))
    })
}

/// The policy, as stored now.
async fn existing_policy(tx: &mut TenantTx, id: Uuid) -> anyhow::Result<PolicyRow> {
    get_policy(tx, id)
        .await?
        .ok_or_else(|| DomainError::not_found("Politique introuvable").into())
}

/// What the administration changes of an allow policy by itself: it switches
/// it off, which only takes access away. Anything else takes a proposal that
/// another administrator approves (ADR-168).
async fn switch_off(
    mut tx: TenantTx,
    access_service: &AccessService,
    access: &AccessContext,
    previous: PolicyRow,
    only_switches_off: bool,
) -> anyhow::Result<()> {
    if !only_switches_off {
        return Err(DomainError::forbidden(
            "Une politique d’autorisation se modifie par une proposition qu’un autre \
             administrateur approuve ; elle se désactive ou se supprime directement",
        )
        .into());
    }
    // Already off: nothing changes, not even what its proposals were made on.
    if !previous.enabled {
        return Ok(());
    }
    switch_off_allow_policy(&mut tx, previous.id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "policy.updated", "policy", previous.id).metadata(json!({
            "effect": previous.effect,
            "enabled": false,
            "previousEnabled": true,
        })),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(access.subject.tenant_id);
    Ok(())
}
