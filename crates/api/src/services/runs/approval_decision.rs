//! Recording a human decision on an approval request, taken by the one who
//! decides it (ADR-059) or whom they named to stand in for them (ADR-071),
//! and deciding whether the run it belongs to can resume.

use chrono::{DateTime, Utc};
use mimorya_agent::runtime::{ApprovalDecision, ApprovalDecisions};
use mimorya_core::DomainError;
use mimorya_db::TenantTx;
use mimorya_db::repositories::approval_delegations::standing_in_for;
use mimorya_db::repositories::approvals::{
    ApprovalDecisionInput, ApprovalRow, decide_approval, expire_approval, get_approval,
    list_approvals_for_run,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::runs::{RunPatch, RunRow, RunStatus, lock_run, update_run};
use serde_json::json;
use uuid::Uuid;

use super::approval_routing::stands_in;
use crate::services::access::AuthIdentity;

#[derive(Debug, Clone)]
pub struct DecisionInput {
    pub approval_id: Uuid,
    pub approved: bool,
    pub comment: Option<String>,
}

pub struct RecordedDecision {
    /// The request had expired: it is recorded as such, and its call is not
    /// run (nobody refused it).
    pub expired: bool,
    pub decided: Option<ApprovalRow>,
    /// Set when every request of the run is settled and the run must resume.
    pub resume: Option<(RunRow, Vec<ApprovalRow>)>,
}

fn already_processed() -> DomainError {
    DomainError::conflict("Cette demande a déjà été traitée")
}

/// The request, if `user_id` may decide it so (ADR-059): whom the agent acts
/// for, the manager named, or whom that manager named to stand in for them
/// while absent (`standing_in_for`, ADR-071). Whom the agent acts for may
/// always withdraw a request a manager decides (refusing runs nothing: the
/// run goes on, even when the manager can no longer decide), never approve
/// it; anyone else finds nothing.
pub fn decidable(
    approval: Option<ApprovalRow>,
    user_id: Uuid,
    standing_in_for: &[Uuid],
    approved: bool,
) -> Result<ApprovalRow, DomainError> {
    match approval {
        Some(a) if a.decider() == Some(user_id) => Ok(a),
        Some(a) if a.routed_to_manager() && a.requested_for_user_id == user_id => {
            if approved {
                Err(DomainError::forbidden(AWAITS_A_MANAGER))
            } else {
                Ok(a)
            }
        }
        Some(a) if stands_in(&a, standing_in_for) => Ok(a),
        _ => Err(DomainError::not_found(NOT_FOUND)),
    }
}

pub const NOT_FOUND: &str = "Demande de validation introuvable";

pub const AWAITS_A_MANAGER: &str =
    "Cette demande attend la validation de votre responsable : vous pouvez seulement la retirer.";

pub async fn record_approval_decision(
    tx: &mut TenantTx,
    identity: AuthIdentity,
    input: &DecisionInput,
    now: DateTime<Utc>,
) -> anyhow::Result<RecordedDecision> {
    // Read in the decision's transaction, at its time: an absence that
    // ended meanwhile no longer lets its delegate decide.
    let standing_in_for = standing_in_for(tx, identity.user_id, now).await?;
    let approval = decidable(
        get_approval(tx, input.approval_id).await?,
        identity.user_id,
        &standing_in_for,
        input.approved,
    )?;
    if approval.status != "pending" {
        return Err(already_processed().into());
    }
    // Concurrent decisions on the same run are serialised: exactly one of them
    // observes the last pending approval and resumes the run.
    let locked = lock_run(tx, approval.run_id).await?;
    // Read again under the run's lock: a decision or an expiry (ADR-061)
    // may have settled it meanwhile.
    let approval = match get_approval(tx, approval.id).await? {
        Some(current) if current.status == "pending" => approval,
        _ => return Err(already_processed().into()),
    };
    // A decision is only accepted once the run is paused: before that, nothing
    // would resume it; after it ended, the action can no longer be carried out.
    let run = match locked {
        Some(run) if run.status() == RunStatus::AwaitingApproval => run,
        other => {
            let finished = other.is_some_and(|r| r.status().is_finished());
            return Err(DomainError::conflict(if finished {
                "Cette exécution est terminée : la demande ne peut plus être traitée."
            } else {
                "La demande de validation est en cours de préparation : réessayez dans un instant."
            })
            .into());
        }
    };
    let expired = approval.expires_at.is_some_and(|at| at <= now);

    let decided = if expired {
        expire_approval(tx, approval.id).await?;
        None
    } else {
        let decided = decide_approval(
            tx,
            ApprovalDecisionInput {
                id: approval.id,
                approved: input.approved,
                decided_by_user_id: identity.user_id,
                comment: input.comment.clone(),
            },
        )
        .await?
        .ok_or_else(already_processed)?;
        Some(decided)
    };

    let action = match (expired, input.approved) {
        (true, _) => "approval.expired",
        (false, true) => "approval.approved",
        (false, false) => "approval.rejected",
    };
    let mut metadata = json!({
        "toolName": approval.tool_name,
        "risk": approval.risk,
        "routing": approval.routing,
        // Whom the agent acts for, refusing a request a manager decides.
        "withdrawn": approval.routed_to_manager()
            && identity.user_id == approval.requested_for_user_id,
    });
    // Decided by whom the absent manager named (ADR-071): that manager.
    let named = approval.decider() == Some(identity.user_id)
        || identity.user_id == approval.requested_for_user_id;
    if let (false, Some(manager)) = (named, approval.approver_user_id) {
        metadata["standingInFor"] = json!(manager);
    }
    append_audit(
        tx,
        AuditEntry::new(
            identity.tenant_id,
            "user",
            action,
            if expired { "failure" } else { "success" },
        )
        .by_user(identity.user_id)
        .on_behalf_of(approval.requested_for_user_id)
        .run(approval.run_id)
        .resource("approval", approval.id)
        .metadata(metadata),
    )
    .await?;

    Ok(RecordedDecision {
        expired,
        decided,
        resume: settle(tx, run).await?,
    })
}

/// Sets the paused run going again once none of its requests waits, after
/// a decision or an expiry (ADR-061): returns it with its requests, to
/// resume; `None` while one still waits.
pub async fn settle(
    tx: &mut TenantTx,
    run: RunRow,
) -> anyhow::Result<Option<(RunRow, Vec<ApprovalRow>)>> {
    let approvals = list_approvals_for_run(tx, run.id).await?;
    let settled = run.status() == RunStatus::AwaitingApproval
        && run.state.is_some()
        && approvals.iter().all(|a| a.status != "pending");
    if !settled {
        return Ok(None);
    }
    let running = RunPatch {
        status: Some(RunStatus::Running),
        ..RunPatch::default()
    };
    update_run(tx, run.id, running).await?;
    Ok(Some((run, approvals)))
}

/// Decisions handed to the agent on resume; expired requests are not run.
pub fn resume_decisions(approvals: &[ApprovalRow]) -> ApprovalDecisions {
    approvals
        .iter()
        .filter_map(|a| {
            let decision = match a.status.as_str() {
                "expired" => ApprovalDecision::expired(),
                "approved" => ApprovalDecision {
                    comment: a.decision_comment.clone(),
                    ..ApprovalDecision::approved()
                },
                "rejected" => ApprovalDecision::rejected(a.decision_comment.clone()),
                _ => return None,
            };
            Some((a.id, decision))
        })
        .collect()
}
