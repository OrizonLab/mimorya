//! Allow policies take two administrators of the access policies (ADR-168):
//! one proposes the policy as it will stand, a new one or a change of an
//! existing one (`propose`); another approves it, which writes it
//! (`approve`), or rejects it; its author may withdraw it (`settle`). Each
//! step runs under the administration lock and is audited.

mod approve;
mod propose;
mod settle;

pub use approve::approve_policy_proposal;
pub use propose::{ProposalInput, propose_policy};
pub use settle::{reject_policy_proposal, withdraw_policy_proposal};

use mimorya_core::DomainError;
use mimorya_db::TenantTx;
use mimorya_db::repositories::policy_proposals::{ProposalRow, get_proposal};
use uuid::Uuid;

const PERMISSION: &str = "policy:manage";

/// The proposal `id`, while it waits for a decision.
pub(super) async fn pending_proposal(tx: &mut TenantTx, id: Uuid) -> anyhow::Result<ProposalRow> {
    let Some(proposal) = get_proposal(tx, id).await? else {
        return Err(DomainError::not_found("Proposition introuvable").into());
    };
    if proposal.status != "pending" {
        return Err(DomainError::conflict("La proposition a déjà été décidée").into());
    }
    Ok(proposal)
}
