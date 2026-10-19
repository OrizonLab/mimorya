//! Approval requests, as listed to the user they are addressed to and as
//! shown in a run's trace.

use mimorya_db::repositories::approvals::ApprovalRow;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::run_stream::ToolRisk;
use super::stored::stored_text;
use crate::http::iso;

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
}

impl Approval {
    pub fn from_row(a: &ApprovalRow) -> anyhow::Result<Self> {
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
