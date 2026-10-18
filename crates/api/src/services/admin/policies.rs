//! Access policies (ADR-022): an administrator creates, changes, disables and
//! deletes deny policies on business permissions; allow policies are shown but
//! not touched here. Conditions are checked before they are stored, since an
//! unreadable one would stop the whole company's authorization from loading.
//! Changes apply at once (cache refreshed).

use mimorya_core::DomainError;
use mimorya_core::authz::Condition;
use mimorya_core::policy_rules::{condition_problem, normalize_policy_permissions};
use mimorya_db::repositories::access_policies::{
    NewDenyPolicy, PolicyPatch, PolicyRow, create_deny_policy, delete_deny_policy, get_policy,
    update_deny_policy,
};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::{Database, TenantTx};
use serde_json::{Value, json};
use uuid::Uuid;

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
    catalog: &[&str],
    input: PolicyInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let permissions = checked_permissions(&input.permissions, catalog)?;
    let condition = checked_condition(&input.condition)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let id = create_deny_policy(
        &mut tx,
        NewDenyPolicy {
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
    catalog: &[&str],
    id: Uuid,
    changes: PolicyChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
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
    let previous = editable_policy(&mut tx, id).await?;
    let metadata = json!({
        "description": changes.description,
        "permissions": permissions,
        "previousPermissions": permissions.as_ref().map(|_| &previous.permissions),
        "condition": condition,
        "previousCondition": condition.as_ref().map(|_| &previous.condition),
        "enabled": changes.enabled,
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
    let previous = editable_policy(&mut tx, id).await?;
    delete_deny_policy(&mut tx, id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "policy.deleted", "policy", id).metadata(json!({
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

fn checked_permissions(requested: &[String], catalog: &[&str]) -> Result<Vec<String>, DomainError> {
    normalize_policy_permissions(requested, catalog).map_err(DomainError::validation)
}

/// The condition as it will be stored, once it passed the checks.
fn checked_condition(condition: &Condition) -> anyhow::Result<Value> {
    if let Some(problem) = condition_problem(condition) {
        return Err(DomainError::validation(problem).into());
    }
    Ok(serde_json::to_value(condition)?)
}

/// The policy, as stored now, if an administrator may change it.
async fn editable_policy(tx: &mut TenantTx, id: Uuid) -> anyhow::Result<PolicyRow> {
    let Some(policy) = get_policy(tx, id).await? else {
        return Err(DomainError::not_found("Politique introuvable").into());
    };
    if policy.effect != "deny" {
        return Err(DomainError::forbidden(
            "Les politiques d’autorisation ne se modifient pas depuis l’administration",
        )
        .into());
    }
    Ok(policy)
}
