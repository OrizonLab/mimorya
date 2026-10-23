//! The events of a run's stream (`GET /runs/{id}/events`): each server-sent
//! event carries its type as the event name and its data as JSON. The engine
//! writes them (`RunEvent`); a test checks that every one of them reads back
//! as its type here.

use mimorya_core::autonomy::ToolRisk as CoreRisk;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::runs::RunStatus;

/// One event of the stream, by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RunStreamEvent {
    RunStarted(RunStarted),
    ConnectorsUnavailable(ConnectorsUnavailable),
    /// Text being written by the model, sent as it comes (not replayed).
    AssistantDelta(AssistantDelta),
    AssistantMessage(AssistantMessage),
    ToolCall(ToolCallEvent),
    ToolResult(ToolResultEvent),
    ApprovalRequested(ApprovalRequestedEvent),
    ApprovalResolved(ApprovalResolvedEvent),
    Verification(VerificationEvent),
    Usage(UsageEvent),
    RunResumed(RunResumed),
    RunQueued(RunQueued),
    RunFinished(RunFinished),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RunStarted {
    pub objective: String,
}

/// Connectors of the company in reach of the user that could not start:
/// their tools are missing from the run (ADR-048).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConnectorsUnavailable {
    /// Their names, as the administration shows them.
    pub connectors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AssistantDelta {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AssistantMessage {
    pub text: String,
    /// The answer, rather than a note written between tool calls.
    #[serde(rename = "final")]
    pub is_final: bool,
}

/// The policy's decision on a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ToolDecision {
    Allow,
    RequireApproval,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolCallEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub title: String,
    pub decision: ToolDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// The tool's risk when it was called (an unknown tool has none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub risk: Option<ToolRisk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolResultEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub summary: String,
    pub is_error: bool,
    pub duration_ms: u64,
}

/// Risk class of a tool, from harmless to irreversible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    Read,
    Draft,
    Write,
    External,
    Irreversible,
}

impl From<CoreRisk> for ToolRisk {
    fn from(risk: CoreRisk) -> Self {
        match risk {
            CoreRisk::Read => Self::Read,
            CoreRisk::Draft => Self::Draft,
            CoreRisk::Write => Self::Write,
            CoreRisk::External => Self::External,
            CoreRisk::Irreversible => Self::Irreversible,
        }
    }
}

impl From<ToolRisk> for CoreRisk {
    fn from(risk: ToolRisk) -> Self {
        match risk {
            ToolRisk::Read => Self::Read,
            ToolRisk::Draft => Self::Draft,
            ToolRisk::Write => Self::Write,
            ToolRisk::External => Self::External,
            ToolRisk::Irreversible => Self::Irreversible,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalRequestedEvent {
    pub approval_id: Uuid,
    pub tool_name: String,
    pub action_summary: String,
    pub risk: ToolRisk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalResolvedEvent {
    pub approval_id: Uuid,
    pub approved: bool,
}

/// Checks of a candidate answer; a failed attempt may be followed by another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct VerificationEvent {
    pub attempt: u32,
    pub passed: bool,
    pub issues: Vec<String>,
}

/// Tokens (and cost) of one model call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageEvent {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// `null` when the model is not priced.
    pub cost_micros: Option<i64>,
}

/// The run was taken up again from its last saved step after a stop of the
/// service (ADR-053).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RunResumed {
    /// Read-only actions under way when the service stopped, whose results
    /// were lost (tool names).
    pub interrupted: Vec<String>,
}

/// The run waits for a worker to execute it (ADR-085): what it does next
/// follows once one claims it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RunQueued {}

/// The run stopped: ended, or paused for a validation (`awaiting_approval`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RunFinished {
    pub status: RunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}
