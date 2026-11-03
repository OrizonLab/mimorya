//! An allow policy approved by another administrator than its author
//! (ADR-168), checked under the administration lock on the stored rules:
//! the approver and the author both still hold `policy:manage` on the whole
//! company, the proposal is pending and not expired, its content passes the
//! rules of today, and a changed policy is as it was when proposed. The
//! policy is written as proposed and applies at once (cache refreshed).

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::authz::PermissionInfo;
use mimorya_db::Database;
use mimorya_db::repositories::access_policies::{
    NewPolicy, create_allow_policy, get_policy, replace_allow_policy,
};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::policy_proposals::{ProposalRow, decide_proposal};
use serde_json::json;
use uuid::Uuid;

use super::super::policy_checks::{checked_condition, checked_grant, stored_condition};
use super::super::{audit_entry, begin_checked_change, holds_company_wide};
use super::{PERMISSION, pending_proposal};
use crate::services::{AccessContext, AccessService};

/// Approves the proposal `id`, which writes its policy: returns the policy.
pub async fn approve_policy_proposal(
    db: &Database,
    access_service: &AccessService,
    access: &AccessContext,
    catalog: &[PermissionInfo],
    id: Uuid,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let (mut tx, engine) = begin_checked_change(db, access, PERMISSION).await?;
    let proposal = pending_proposal(&mut tx, id).await?;
    if proposal.proposed_by_user_id == access.subject.user_id {
        return Err(DomainError::forbidden(
            "Une proposition est approuvée par un autre administrateur que son auteur",
        )
        .into());
    }
    if proposal.expires_at <= Utc::now() {
        return Err(DomainError::conflict(
            "La proposition a expiré : il faut en faire une nouvelle",
        )
        .into());
    }
    if !holds_company_wide(&mut tx, &engine, proposal.proposed_by_user_id, PERMISSION).await? {
        return Err(DomainError::conflict(
            "Son auteur n’administre plus les politiques d’accès : la proposition ne peut plus \
             être approuvée",
        )
        .into());
    }
    let content = approved_content(access, &proposal, catalog)?;
    let (policy_id, previous) = match proposal.policy_id {
        None => (create_allow_policy(&mut tx, content.clone()).await?, None),
        Some(policy_id) => {
            let previous = get_policy(&mut tx, policy_id).await?;
            let proposed_on = proposal
                .policy_updated_at
                .ok_or_else(|| anyhow::anyhow!("a policy change proposal without its base"))?;
            if !replace_allow_policy(&mut tx, policy_id, proposed_on, content.clone()).await? {
                return Err(DomainError::conflict(
                    "La politique a changé depuis la proposition : il faut en faire une nouvelle",
                )
                .into());
            }
            (policy_id, previous)
        }
    };
    decide_proposal(&mut tx, id, "approved", access.subject.user_id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "policy_proposal.approved", "policy_proposal", id).metadata(json!({
            "policyId": policy_id,
            "proposedByUserId": proposal.proposed_by_user_id,
        })),
    )
    .await?;
    let action = if previous.is_some() {
        "policy.updated"
    } else {
        "policy.created"
    };
    append_audit(
        &mut tx,
        audit_entry(access, action, "policy", policy_id).metadata(json!({
            "effect": "allow",
            "proposalId": id,
            "proposedByUserId": proposal.proposed_by_user_id,
            "description": content.description,
            "permissions": content.permissions,
            "condition": content.condition,
            "enabled": content.enabled,
            "previousDescription": previous.as_ref().map(|p| &p.description),
            "previousPermissions": previous.as_ref().map(|p| &p.permissions),
            "previousCondition": previous.as_ref().map(|p| &p.condition),
            "previousEnabled": previous.as_ref().map(|p| p.enabled),
        })),
    )
    .await?;
    tx.commit().await?;
    access_service.invalidate(access.subject.tenant_id);
    Ok(policy_id)
}

/// The policy a proposal writes, checked again: the rules may have changed
/// since it was made.
fn approved_content(
    access: &AccessContext,
    proposal: &ProposalRow,
    catalog: &[PermissionInfo],
) -> anyhow::Result<NewPolicy> {
    let outdated = |problem: &str| {
        DomainError::conflict(format!(
            "La proposition n’est plus acceptée ({problem}) : il faut en faire une nouvelle"
        ))
    };
    let permissions =
        checked_grant(&proposal.permissions, catalog).map_err(|e| outdated(&e.message))?;
    let condition =
        stored_condition(&proposal.condition).map_err(|_| outdated("condition illisible"))?;
    Ok(NewPolicy {
        tenant_id: access.subject.tenant_id,
        description: proposal.description.clone(),
        permissions,
        condition: checked_condition(&condition)?,
        enabled: proposal.enabled,
    })
}
