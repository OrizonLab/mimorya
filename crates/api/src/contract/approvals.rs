//! Approval requests, as listed to whom the agent acts for and to the
//! manager who decides them (ADR-059), and as shown in a run's trace, and
//! the decision taken on one.

use mimorya_db::repositories::approval_views::ApprovalView;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::run_stream::ToolRisk;
use super::stored::stored_text;
use crate::http::{iso, trimmed_opt};

/// An action of the agent waiting for (or given) a human decision.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: Uuid,
    pub run_id: Uuid,
    pub tool_name: String,
    pub action_summary: String,
    /// The validated input, executed as is once approved.
    #[ts(type = "unknown")]
    pub payload: Value,
    pub risk: ToolRisk,
    pub reason: String,
    pub status: String,
    pub decision_comment: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
    /// After it, nobody may decide it any more.
    pub expires_at: Option<String>,
    /// Whom the agent acts for.
    pub requested_for: ApprovalPerson,
    /// Who decides it: whom the agent acts for, or the manager named when it
    /// was made (ADR-059); `null` once that manager's account is gone.
    pub approver: Option<ApprovalPerson>,
    pub decided_by: Option<ApprovalPerson>,
    /// Whether the reader may decide it now: approve or refuse it.
    pub can_decide: bool,
    /// Whether the reader may withdraw it now: made for them, a manager
    /// decides it (ADR-059). Withdrawn, it is refused.
    pub can_withdraw: bool,
}

/// Someone a request names.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalPerson {
    pub id: Uuid,
    pub display_name: String,
}

impl Approval {
    pub fn from_view(
        view: &ApprovalView,
        can_decide: bool,
        can_withdraw: bool,
    ) -> anyhow::Result<Self> {
        let a = &view.approval;
        let person = |id: Option<Uuid>, name: &Option<String>| match (id, name) {
            (Some(id), Some(name)) => Some(ApprovalPerson {
                id,
                display_name: name.clone(),
            }),
            _ => None,
        };
        let requested_for = ApprovalPerson {
            id: a.requested_for_user_id,
            display_name: view.requested_for_name.clone(),
        };
        let approver = if a.routed_to_manager() {
            person(a.approver_user_id, &view.approver_name)
        } else {
            person(
                Some(requested_for.id),
                &Some(requested_for.display_name.clone()),
            )
        };
        Ok(Self {
            id: a.id,
            run_id: a.run_id,
            tool_name: a.tool_name.clone(),
            action_summary: a.action_summary.clone(),
            payload: a.payload.clone(),
            risk: stored_text("approval risk", &a.risk)?,
            reason: a.reason.clone(),
            status: a.status.clone(),
            decision_comment: a.decision_comment.clone(),
            created_at: iso(&a.created_at),
            decided_at: a.decided_at.as_ref().map(iso),
            expires_at: a.expires_at.as_ref().map(iso),
            requested_for,
            approver,
            decided_by: person(a.decided_by_user_id, &view.decided_by_name),
            can_decide,
            can_withdraw,
        })
    }
}

/// `GET /approvals`.
#[derive(Debug, Serialize, TS)]
pub struct ApprovalList {
    pub approvals: Vec<Approval>,
}

/// `POST /approvals/{id}/decision`.
#[derive(Debug, Serialize, TS)]
pub struct ApprovalDecided {
    pub approval: Approval,
    /// True when this decision was the run's last one awaited: the run resumes.
    pub resumed: bool,
}

/// Body of `POST /approvals/{id}/decision`.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct ApprovalVerdict {
    pub approved: bool,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 500))]
    #[ts(optional)]
    pub comment: Option<String>,
}
