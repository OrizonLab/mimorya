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
        }
    }
}
