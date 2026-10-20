//! Recording a human decision on an approval request, and deciding whether the
//! run it belongs to can resume.

use chrono::{DateTime, Utc};
use mimorya_agent::runtime::{ApprovalDecision, ApprovalDecisions};
use mimorya_core::DomainError;
use mimorya_db::TenantTx;
use mimorya_db::repositories::approvals::{
    ApprovalDecisionInput, ApprovalRow, decide_approval, expire_approval, get_approval,
    list_approvals_for_run,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::runs::{RunPatch, RunRow, RunStatus, lock_run, update_run};
use serde_json::json;
use uuid::Uuid;

use crate::services::access::AuthIdentity;

#[derive(Debug, Clone)]
pub struct DecisionInput {
    pub approval_id: Uuid,
    pub approved: bool,
    pub comment: Option<String>,
}

pub struct RecordedDecision {
    /// The request had expired: it is recorded as such and counts as a refusal.
    pub expired: bool,
    pub decided: Option<ApprovalRow>,
    /// Set when every request of the run is settled and the run must resume.
    pub resume: Option<(RunRow, Vec<ApprovalRow>)>,
}

fn already_processed() -> DomainError {
    DomainError::conflict("Cette demande a déjà été traitée")
}

pub async fn record_approval_decision(
    tx: &mut TenantTx,
    identity: AuthIdentity,
    input: &DecisionInput,
    now: DateTime<Utc>,
) -> anyhow::Result<RecordedDecision> {
    // MVP rule: a user validates the actions requested on their own behalf.
    let approval = get_approval(tx, input.approval_id)
        .await?
        .filter(|a| a.requested_for_user_id == identity.user_id)
        .ok_or_else(|| DomainError::not_found("Demande de validation introuvable"))?;
    if approval.status != "pending" {
        return Err(already_processed().into());
    }
    // Concurrent decisions on the same run are serialised: exactly one of them
    // observes the last pending approval and resumes the run.
    // A decision is only accepted once the run is paused: before that, nothing
    // would resume it; after it ended, the action can no longer be carried out.
    let run = match lock_run(tx, approval.run_id).await? {
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
    append_audit(
        tx,
        AuditEntry::new(
            identity.tenant_id,
            "user",
            action,
            if expired { "failure" } else { "success" },
        )
        .by_user(identity.user_id)
        .run(approval.run_id)
        .resource("approval", approval.id)
        .metadata(json!({ "toolName": approval.tool_name, "risk": approval.risk })),
    )
    .await?;

    let approvals = list_approvals_for_run(tx, approval.run_id).await?;
    let can_resume = run.state.is_some() && approvals.iter().all(|a| a.status != "pending");
    if !can_resume {
        return Ok(RecordedDecision {
            expired,
            decided,
            resume: None,
        });
    }
    update_run(
        tx,
        run.id,
        RunPatch {
            status: Some(RunStatus::Running),
            ..RunPatch::default()
        },
    )
    .await?;
    Ok(RecordedDecision {
        expired,
        decided,
        resume: Some((run, approvals)),
    })
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
