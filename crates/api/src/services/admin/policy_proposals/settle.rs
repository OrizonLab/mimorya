//! A proposal turned down (ADR-168): rejected by another administrator
//! than its author, or withdrawn by its author. Nothing is written but the
//! decision and its audit entry.

use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::policy_proposals::{ProposalRow, decide_proposal};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::super::{audit_entry, begin_change};
use super::{PERMISSION, pending_proposal};
use crate::services::AccessContext;

/// Rejects the proposal `id`: another administrator than its author.
pub async fn reject_policy_proposal(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let proposal = pending_proposal(&mut tx, id).await?;
    if proposal.proposed_by_user_id == access.subject.user_id {
        return Err(DomainError::forbidden(
            "Une proposition est refusée par un autre administrateur que son auteur : retirez \
             la vôtre",
        )
        .into());
    }
    settle(tx, access, &proposal, "rejected").await
}

/// Withdraws the proposal `id`: its author only.
pub async fn withdraw_policy_proposal(
    db: &Database,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let proposal = pending_proposal(&mut tx, id).await?;
    if proposal.proposed_by_user_id != access.subject.user_id {
        return Err(DomainError::forbidden("Seul son auteur retire une proposition").into());
    }
    settle(tx, access, &proposal, "withdrawn").await
}

async fn settle(
    mut tx: TenantTx,
    access: &AccessContext,
    proposal: &ProposalRow,
    status: &str,
) -> anyhow::Result<()> {
    decide_proposal(&mut tx, proposal.id, status, access.subject.user_id).await?;
    let action = format!("policy_proposal.{status}");
    append_audit(
        &mut tx,
        audit_entry(access, &action, "policy_proposal", proposal.id).metadata(json!({
            "policyId": proposal.policy_id,
            "proposedByUserId": proposal.proposed_by_user_id,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
