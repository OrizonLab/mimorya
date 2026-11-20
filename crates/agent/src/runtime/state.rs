//! Everything needed to resume a run after an interruption or a human
//! validation. Serialisable as JSON (stored in `agent_runs.state`).

use mimorya_core::tools::Citation;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::ports::SkillSummary;
use crate::llm::{AdvisorTool, LlmMessage, LlmToolResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunUsageTotals {
    pub steps: u32,
    pub tool_calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// What the calls cost at the platform's prices.
    pub cost_micros: i64,
    /// What they cost the company (ADR-212): their cost on its own key, the
    /// amount re-invoiced on the operator's (ADR-208). A run checkpointed
    /// before counts its earlier calls at their cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charged_micros: Option<i64>,
}

impl RunUsageTotals {
    /// What the calls cost the company: what budgets and limits weigh.
    pub fn charged(&self) -> i64 {
        self.charged_micros.unwrap_or(self.cost_micros)
    }

    /// Adds a call, or calls, that cost `cost_micros` and were charged
    /// `charged_micros`.
    pub fn add_cost(&mut self, cost_micros: i64, charged_micros: i64) {
        let charged = self.charged();
        self.cost_micros += cost_micros;
        self.charged_micros = Some(charged + charged_micros);
    }
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
    /// The skills its first turn listed, chosen by the company's decision
    /// (ADR-213, D2): its sub-agents are listed the same, even those a
    /// person validates after a pause.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills_shown: Vec<SkillSummary>,
    /// The run read an external content said to hold instructions for an
    /// AI, or one the detection could not read (ADR-213, D6): each later
    /// action waits for a person.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external_suspect: bool,
    /// Of which an external content was said to hold such instructions
    /// (not only unread): a sub-agent's report is then marked so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub instructions_detected: bool,
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
    /// Anthropic's advisor in the run's calls (step 3).
    #[serde(default)]
    pub advisor: AdvisorState,
    /// Whether the last call paused, a consultation still running: the next
    /// one carries the advisor whatever the bounds.
    #[serde(default)]
    pub advisor_pending: bool,
    /// The consultations of the current turn, which Anthropic bounds: the
    /// platform's message opens a turn.
    #[serde(default)]
    pub turn_consultations: u32,
    /// Whether a paused turn went on past the run's budget: once a run.
    #[serde(default)]
    pub paused_past_budget: bool,
}

/// Anthropic's advisor in a run's calls: decided at its first call, kept
/// unchanged after, and withdrawn once for good.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "state", content = "tool")]
pub enum AdvisorState {
    /// The run has made no call yet.
    #[default]
    Undecided,
    /// Not given: the company does not have it, the run started on the
    /// escalation model, or a bound was reached.
    Off,
    /// Given to every call, unchanged.
    Given(AdvisorTool),
    /// Withdrawn after it was given: a bound reached, or the company's
    /// escalation changed.
    Withdrawn,
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
            skills_shown: Vec::new(),
            external_suspect: false,
            instructions_detected: false,
        }
    }
}
