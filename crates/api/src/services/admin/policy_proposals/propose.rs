//! A proposal of an allow policy as it will stand (ADR-168): its content
//! passes the rules of a policy that grants, and a change names an allow
//! policy and changes something of it. It waits `GRANT_PROPOSAL_DAYS` for
//! another administrator.

use chrono::{DateTime, Duration, Utc};
use mimorya_core::DomainError;
use mimorya_core::authz::{Condition, PermissionInfo};
use mimorya_core::policy_rules::GRANT_PROPOSAL_DAYS;
use mimorya_db::repositories::access_policies::{PolicyRow, get_policy};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::policy_proposals::{
    MAX_PENDING_PROPOSALS, NewProposal, count_waiting_proposals, create_proposal,
};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::super::policy_checks::{checked_condition, checked_grant};
use super::super::{audit_entry, begin_change};
use super::PERMISSION;
use crate::services::AccessContext;

/// An allow policy as it will stand: a new one, or `policy_id` changed,
/// from its version `policy_updated_at` when given.
pub struct ProposalInput {
    pub policy_id: Option<Uuid>,
    pub policy_updated_at: Option<DateTime<Utc>>,
    pub description: String,
    pub permissions: Vec<String>,
    pub condition: Condition,
    pub enabled: bool,
}

pub async fn propose_policy(
    db: &Database,
    access: &AccessContext,
    catalog: &[PermissionInfo],
    input: ProposalInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    if input.policy_id.is_none() && input.policy_updated_at.is_some() {
        return Err(DomainError::validation(
            "Une version de politique ne se donne qu’avec la politique qu’elle désigne",
        )
        .into());
    }
    let permissions = checked_grant(&input.permissions, catalog)?;
    let condition = checked_condition(&input.condition)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let changed = match input.policy_id {
        Some(id) => Some(changed_policy(&mut tx, id).await?),
        None => None,
    };
    // Changed since its author read it, a policy is not proposed on: a
    // switch-off made meanwhile would be undone unseen (to the millisecond,
    // as the API gives it).
    if let (Some(policy), Some(seen)) = (&changed, input.policy_updated_at)
        && policy.updated_at.timestamp_millis() != seen.timestamp_millis()
    {
        return Err(DomainError::conflict(
            "La politique a changé depuis que vous l’avez ouverte : annulez, puis rouvrez-la \
             avant de proposer",
        )
        .into());
    }
    if changed.as_ref().is_some_and(|policy| {
        policy.description == input.description
            && policy.permissions == permissions
            && policy.condition == condition
            && policy.enabled == input.enabled
    }) {
        return Err(DomainError::validation("La proposition ne change rien à la politique").into());
    }
    if count_waiting_proposals(&mut tx).await? >= MAX_PENDING_PROPOSALS {
        return Err(DomainError::conflict(format!(
            "{MAX_PENDING_PROPOSALS} propositions attendent déjà une décision : décidez-en avant \
             d’en faire d’autres"
        ))
        .into());
    }
    let expires_at = Utc::now() + Duration::days(GRANT_PROPOSAL_DAYS);
    let id = create_proposal(
        &mut tx,
        NewProposal {
            tenant_id: access.subject.tenant_id,
            policy_id: input.policy_id,
            policy_updated_at: changed.map(|policy| policy.updated_at),
            description: input.description.clone(),
            permissions: permissions.clone(),
            condition: condition.clone(),
            enabled: input.enabled,
            proposed_by_user_id: access.subject.user_id,
            expires_at,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "policy_proposal.created", "policy_proposal", id).metadata(json!({
            "policyId": input.policy_id,
            "description": input.description,
            "permissions": permissions,
            "condition": condition,
            "enabled": input.enabled,
            "expiresAt": expires_at.to_rfc3339(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// The allow policy a proposal changes, named in its body.
async fn changed_policy(tx: &mut TenantTx, id: Uuid) -> anyhow::Result<PolicyRow> {
    let Some(policy) = get_policy(tx, id).await? else {
        return Err(DomainError::unknown("Politique inconnue", "policy", id).into());
    };
    if policy.effect != "allow" {
        return Err(DomainError::validation(
            "Une politique de refus se modifie directement, sans proposition",
        )
        .into());
    }
    Ok(policy)
}
