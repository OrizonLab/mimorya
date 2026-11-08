//! What the assistant did for the person, in their export (ADR-187): the
//! runs made on their behalf with the tools each called, the validations
//! asked of them, those they gave on others' runs, and what was consumed
//! for them month by month.

use std::collections::HashMap;

use mimorya_db::repositories::personal_activity::{
    ExportedApproval, ExportedDecision, ExportedRun, ExportedToolCall, ExportedUsage,
};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::iso_opt;
use crate::http::iso;

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportRun {
    pub id: Uuid,
    pub conversation_id: Option<Uuid>,
    /// The run that delegated to this one, for a sub-agent's (ADR-098).
    pub parent_run_id: Option<Uuid>,
    pub objective: String,
    pub status: String,
    pub channel: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub tool_calls: Vec<ExportToolCall>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportToolCall {
    pub tool: String,
    /// `allow`, `require_approval` or `deny`.
    pub decision: String,
    pub status: String,
    #[ts(type = "unknown")]
    pub input: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportApproval {
    pub run_id: Uuid,
    pub tool: String,
    pub action_summary: String,
    #[ts(type = "unknown")]
    pub payload: Value,
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub decision_comment: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportDecision {
    pub tool: String,
    pub status: String,
    pub decided_at: Option<String>,
    pub decision_comment: Option<String>,
}

/// What the assistant consumed for the person in a month, of one kind.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportUsage {
    /// The month's first instant.
    pub month: String,
    /// `llm`, `embedding` or `tool`.
    pub kind: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micros: i64,
}

/// The person's runs, each with the tools it called, in the order called.
pub fn run_exports(runs: Vec<ExportedRun>, calls: Vec<ExportedToolCall>) -> Vec<ExportRun> {
    let mut exported: Vec<ExportRun> = runs
        .into_iter()
        .map(|run| ExportRun {
            id: run.id,
            conversation_id: run.conversation_id,
            parent_run_id: run.parent_run_id,
            objective: run.objective,
            status: run.status,
            channel: run.channel,
            created_at: iso(&run.created_at),
            finished_at: iso_opt(run.finished_at),
            tool_calls: Vec::new(),
        })
        .collect();
    let places: HashMap<Uuid, usize> = exported
        .iter()
        .enumerate()
        .map(|(place, run)| (run.id, place))
        .collect();
    for call in calls {
        if let Some(&place) = places.get(&call.run_id) {
            exported[place].tool_calls.push(ExportToolCall {
                tool: call.tool_name,
                decision: call.decision,
                status: call.status,
                input: call.input,
                created_at: iso(&call.created_at),
            });
        }
    }
    exported
}

impl From<ExportedApproval> for ExportApproval {
    fn from(approval: ExportedApproval) -> Self {
        Self {
            run_id: approval.run_id,
            tool: approval.tool_name,
            action_summary: approval.action_summary,
            payload: approval.payload,
            status: approval.status,
            created_at: iso(&approval.created_at),
            decided_at: iso_opt(approval.decided_at),
            decision_comment: approval.decision_comment,
        }
    }
}

impl From<ExportedDecision> for ExportDecision {
    fn from(decision: ExportedDecision) -> Self {
        Self {
            tool: decision.tool_name,
            status: decision.status,
            decided_at: iso_opt(decision.decided_at),
            decision_comment: decision.decision_comment,
        }
    }
}

impl From<ExportedUsage> for ExportUsage {
    fn from(usage: ExportedUsage) -> Self {
        Self {
            month: iso(&usage.month),
            kind: usage.kind,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cost_micros: usage.cost_micros,
        }
    }
}
