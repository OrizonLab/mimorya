//! Agent runs as the web client sees them: list, detail with its trace, and
//! the run records stored as JSON (usage, budget, verification).

use mimorya_agent::runtime::VerificationReport;
use mimorya_db::repositories::run_events::RunEventRow;
use mimorya_db::repositories::runs::{self, RunRow};
use mimorya_db::repositories::tool_invocations::ToolInvocationRow;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::approvals::Approval;
use super::run_stream::ToolDecision;
use super::stored::{stored, stored_text};
use crate::http::iso;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

impl From<runs::RunStatus> for RunStatus {
    fn from(status: runs::RunStatus) -> Self {
        match status {
            runs::RunStatus::Queued => Self::Queued,
            runs::RunStatus::Running => Self::Running,
            runs::RunStatus::AwaitingApproval => Self::AwaitingApproval,
            runs::RunStatus::Completed => Self::Completed,
            runs::RunStatus::Failed => Self::Failed,
            runs::RunStatus::Cancelled => Self::Cancelled,
        }
    }
}

/// What a run has consumed so far (`agent_runs.usage`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunUsage {
    pub steps: u32,
    pub tool_calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Estimated cost, in millionths of the currency unit.
    pub cost_micros: i64,
}

/// The limits a run stops at (`agent_runs.budget`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunBudget {
    pub max_steps: u32,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_cost_micros: i64,
    pub max_duration_ms: u64,
}

/// Checks of a run's final answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    pub passed: bool,
    pub issues: Vec<String>,
    /// True when a model review ran and gave a usable verdict.
    pub judged: bool,
}

impl From<&VerificationReport> for Verification {
    fn from(report: &VerificationReport) -> Self {
        Self {
            passed: report.passed,
            issues: report.issues.clone(),
            judged: report.judged,
        }
    }
}

/// Who works a run: the agent the user talks to, or one of its sub-agents
/// (ADR-098).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentProfile {
    Main,
    Research,
    Analysis,
    Planning,
    Quality,
}

/// A run in the list of the user's runs, or of a run's sub-agents.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunListItem {
    pub id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub objective: String,
    pub agent_profile: AgentProfile,
    pub status: RunStatus,
    pub usage: RunUsage,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

impl RunListItem {
    pub fn from_row(run: &RunRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: run.id,
            conversation_id: run.conversation_id,
            objective: run.objective.clone(),
            agent_profile: stored_text("agent profile", &run.agent_profile)?,
            status: run.status().into(),
            usage: stored("usage", &run.usage)?,
            error: run.error.clone(),
            created_at: iso(&run.created_at),
            finished_at: run.finished_at.as_ref().map(iso),
        })
    }
}

/// `GET /runs`.
#[derive(Debug, Serialize, TS)]
pub struct RunList {
    pub runs: Vec<RunListItem>,
}

/// A run of a conversation, listed with its messages.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunRef {
    pub id: Uuid,
    pub status: RunStatus,
    pub created_at: String,
}

impl From<&RunRow> for RunRef {
    fn from(run: &RunRow) -> Self {
        Self {
            id: run.id,
            status: run.status().into(),
            created_at: iso(&run.created_at),
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    pub id: Uuid,
    pub conversation_id: Option<Uuid>,
    /// The run that handed it its task, for a sub-agent's (ADR-098).
    pub parent_run_id: Option<Uuid>,
    pub agent_profile: AgentProfile,
    pub objective: String,
    pub status: RunStatus,
    pub autonomy_level: String,
    pub usage: RunUsage,
    pub budget: RunBudget,
    pub verification: Option<Verification>,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

impl RunView {
    pub fn from_row(run: &RunRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: run.id,
            conversation_id: run.conversation_id,
            parent_run_id: run.parent_run_id,
            agent_profile: stored_text("agent profile", &run.agent_profile)?,
            objective: run.objective.clone(),
            status: run.status().into(),
            autonomy_level: run.autonomy_level.clone(),
            usage: stored("usage", &run.usage)?,
            budget: stored("budget", &run.budget)?,
            verification: run
                .verification
                .as_ref()
                .map(|v| stored("verification", v))
                .transpose()?,
            error: run.error.clone(),
            created_at: iso(&run.created_at),
            finished_at: run.finished_at.as_ref().map(iso),
        })
    }
}

/// A tool call of a run, with the decision taken on it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvocation {
    pub id: Uuid,
    pub tool_name: String,
    pub decision: ToolDecision,
    pub decision_reason: Option<String>,
    pub status: String,
    pub output_summary: Option<String>,
    pub duration_ms: Option<i32>,
    pub created_at: String,
}

impl ToolInvocation {
    pub fn from_row(t: &ToolInvocationRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: t.id,
            tool_name: t.tool_name.clone(),
            decision: stored_text("tool decision", &t.decision)?,
            decision_reason: t.decision_reason.clone(),
            status: t.status.clone(),
            output_summary: t.output_summary.clone(),
            duration_ms: t.duration_ms,
            created_at: iso(&t.created_at),
        })
    }
}

/// An event of a run's journal, as replayed by its event stream.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunEventRecord {
    pub seq: i32,
    #[serde(rename = "type")]
    pub event_type: String,
    #[ts(type = "unknown")]
    pub data: Value,
    pub created_at: String,
}

impl From<&RunEventRow> for RunEventRecord {
    fn from(e: &RunEventRow) -> Self {
        Self {
            seq: e.seq,
            event_type: e.event_type.clone(),
            data: e.data.clone(),
            created_at: iso(&e.created_at),
        }
    }
}

/// `GET /runs/{id}`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    pub run: RunView,
    pub tools: Vec<ToolInvocation>,
    pub approvals: Vec<Approval>,
    pub events: Vec<RunEventRecord>,
    /// The sub-agents it handed tasks to, in order (ADR-098).
    pub sub_runs: Vec<RunListItem>,
}
