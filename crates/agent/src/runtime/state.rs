//! Everything needed to resume a run after an interruption or a human
//! validation. Serialisable as JSON (stored in `agent_runs.state`).

use mimorya_core::tools::Citation;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::llm::{LlmMessage, LlmToolResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunUsageTotals {
    pub steps: u32,
    pub tool_calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApproval {
    pub approval_id: Uuid,
    pub tool_call_id: String,
    pub tool_name: String,
    /// Validated input, executed as-is once approved (cannot change after validation).
    pub input: Value,
    /// What the person deciding read of the action, its input's keys in one
    /// order (stored as `jsonb`, a checkpoint comes back with them
    /// reordered). Once approved, a tool that no longer describes the input
    /// so (its declaration changed since, ADR-112) does not run it. Absent
    /// from runs paused before it was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCalls {
    pub completed_results: Vec<LlmToolResult>,
    pub approvals: Vec<PendingApproval>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCheckpoint {
    pub version: u32,
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub usage: RunUsageTotals,
    pub correction_attempts: u32,
    pub evidence: Vec<Citation>,
    pub elapsed_ms: u64,
    /// Set while the run waits for human validation of some tool calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingCalls>,
    /// Times the run was taken up again after the service stopped (ADR-053).
    #[serde(default)]
    pub resumptions: u32,
    /// The quality criteria of the skills loaded (ADR-106), in the order
    /// they were loaded, each once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_criteria: Vec<String>,
    /// Where the run stands with the company's model escalation (ADR-207):
    /// kept so that a run another process takes up stays on the same model.
    #[serde(default, skip_serializing_if = "EscalationState::is_untouched")]
    pub escalation: EscalationState,
    /// Whether the operator's supply stopped a call of the run (ADR-208):
    /// said once a run.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supply_stopped: bool,
}

/// A run's model escalation (ADR-207).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EscalationState {
    /// Whether the run's model calls go to the escalation model.
    pub escalated: bool,
    /// The calls made to it.
    pub calls: u32,
    /// Whether the run escalated after a failed review: once a run.
    pub after_failure_used: bool,
    /// Whether a bound (the month's budget, the run's escalations) stopped
    /// an escalation: said once a run.
    pub limited: bool,
    /// Whether a skill the run loaded asks for the escalation model.
    #[serde(default)]
    pub skill_asked: bool,
    /// Whether that was heeded: once a run.
    #[serde(default)]
    pub skill_heeded: bool,
}

impl EscalationState {
    fn is_untouched(&self) -> bool {
        self == &Self::default()
    }
}

impl RunCheckpoint {
    pub fn new(system: String, messages: Vec<LlmMessage>) -> Self {
        Self {
            version: 1,
            system,
            messages,
            usage: RunUsageTotals::default(),
            correction_attempts: 0,
            evidence: Vec::new(),
            elapsed_ms: 0,
            pending: None,
            resumptions: 0,
            skill_criteria: Vec::new(),
            escalation: EscalationState::default(),
            supply_stopped: false,
        }
    }
}
