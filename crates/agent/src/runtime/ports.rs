//! Ports the engine depends on. The API provides PostgreSQL implementations;
//! `testing` provides in-memory ones. The engine never touches a database.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mimorya_core::authz::{AuthorizationEngine, Sensitivity, Subject};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::memory::{MemoryKind, MemoryReadFilter, MemoryScope, MemorySource, MemoryStatus};
use mimorya_core::tools::DeliverableService;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::state::RunCheckpoint;
use super::types::RunOutcome;

/// What a run reports while it progresses (persisted, then streamed to clients).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RunEvent {
    RunStarted {
        objective: String,
    },
    /// Connectors of the company that could not start, in reach of the
    /// user (ADR-048): their tools are missing from the run.
    ConnectorsUnavailable {
        connectors: Vec<String>,
    },
    AssistantMessage {
        text: String,
        #[serde(rename = "final")]
        is_final: bool,
    },
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        title: String,
        /// `allow`, `require_approval` or `deny`.
        decision: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// The tool's risk when it was called (an unknown tool has none): what
        /// a stop of the service may have left half done (ADR-053).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        risk: Option<ToolRisk>,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        summary: String,
        is_error: bool,
        duration_ms: u64,
    },
    ApprovalRequested {
        approval_id: Uuid,
        tool_name: String,
        action_summary: String,
        risk: ToolRisk,
    },
    ApprovalResolved {
        approval_id: Uuid,
        approved: bool,
    },
    Verification {
        attempt: u32,
        passed: bool,
        issues: Vec<String>,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        /// `None` when the model is not priced.
        cost_micros: Option<i64>,
    },
    /// Taken up again from its last saved step after the service stopped
    /// (ADR-053).
    RunResumed {
        /// Read-only actions under way when it stopped, whose results were
        /// lost: the model may run them again.
        interrupted: Vec<String>,
    },
    /// Waiting for a worker to execute it (ADR-085): written by an `api`
    /// process as it queues the run, never by the engine.
    RunQueued {},
    RunFinished {
        /// `completed`, `awaiting_approval`, `failed` or `cancelled`.
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

impl RunEvent {
    /// Event name and payload (without the `type` tag), as stored and streamed.
    pub fn into_parts(self) -> (String, Value) {
        let mut value = serde_json::to_value(self).expect("serialisable event");
        let event_type = value
            .as_object_mut()
            .and_then(|map| map.remove("type"))
            .and_then(|t| t.as_str().map(str::to_string))
            .unwrap_or_default();
        (event_type, value)
    }
}

#[async_trait]
pub trait RunEventSink: Send + Sync {
    /// Persists and broadcasts an event.
    async fn emit(&self, event: RunEvent) -> anyhow::Result<()>;
    /// Broadcasts streamed text immediately, without persisting it.
    fn publish_delta(&self, text: &str);
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineAuditEntry {
    pub action: String,
    /// `success`, `denied` or `failure`.
    pub outcome: &'static str,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub reason: Option<String>,
    pub metadata: Value,
}

/// Audit entries recorded by the engine are attributed to "agent on behalf of user".
#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn record(&self, entry: EngineAuditEntry) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolInvocationRecord {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Value,
    /// `allow`, `require_approval` or `deny`.
    pub decision: &'static str,
    pub decision_reason: Option<String>,
    /// `succeeded`, `failed`, `denied`, `awaiting_approval` or `rejected`.
    pub status: &'static str,
    pub output_summary: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub approval_id: Option<Uuid>,
}

#[async_trait]
pub trait ToolInvocationLog: Send + Sync {
    async fn record(&self, entry: ToolInvocationRecord) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub tool_call_id: String,
    pub action_summary: String,
    pub payload: Value,
    pub risk: ToolRisk,
    pub reason: String,
}

/// Where a request for validation went (ADR-059).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalPlacement {
    /// Waiting for a decision.
    Pending(Uuid),
    /// Nobody may validate it: the call does not run, for this reason (in
    /// French, for the model and the people reading the trace).
    Unroutable(String),
}

#[async_trait]
pub trait ApprovalStore: Send + Sync {
    async fn create(&self, request: ApprovalRequest) -> anyhow::Result<ApprovalPlacement>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    /// `llm`, `embedding` or `tool`.
    pub kind: &'static str,
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_micros: i64,
}

#[async_trait]
pub trait UsageSink: Send + Sync {
    async fn record(&self, usage: UsageRecord) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryHit {
    pub id: Uuid,
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub org_unit_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewMemory {
    pub scope: MemoryScope,
    pub owner_user_id: Option<Uuid>,
    pub org_unit_id: Option<Uuid>,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub sensitivity: Sensitivity,
    pub status: MemoryStatus,
    pub source: MemorySource,
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Searches only within `filter` (pre-filtering, never post-filtering).
    async fn search(
        &self,
        filter: &MemoryReadFilter,
        query: &str,
        limit: u32,
        clearance: Sensitivity,
    ) -> anyhow::Result<Vec<MemoryHit>>;
    async fn create(&self, memory: NewMemory) -> anyhow::Result<Uuid>;
    async fn mark_used(&self, ids: &[Uuid]) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillContent {
    pub name: String,
    pub version: i32,
    pub content: String,
}

#[async_trait]
pub trait SkillStore: Send + Sync {
    async fn list_active(&self) -> anyhow::Result<Vec<SkillSummary>>;
    async fn get_active(&self, name: &str) -> anyhow::Result<Option<SkillContent>>;
}

#[async_trait]
pub trait RunStore: Send + Sync {
    /// Saves progress so that the run can resume after a pause or a restart.
    async fn checkpoint(&self, state: &RunCheckpoint) -> anyhow::Result<()>;
    /// Durably records how the run ended or paused (status, answer, audit).
    /// The runtime calls it before announcing `run_finished`: clients reload
    /// on that event and must find the outcome saved, and a decision on an
    /// approval must find the run paused.
    async fn finish(&self, outcome: &RunOutcome) -> anyhow::Result<()>;
}

/// The rights a run acts with at one step: the company's rules and the user's
/// assignments and attributes.
#[derive(Clone)]
pub struct CurrentAccess {
    pub engine: Arc<AuthorizationEngine>,
    pub subject: Arc<Subject>,
}

/// What a run may do at one step.
#[derive(Clone)]
pub enum StepAccess {
    /// The rights it acts with.
    Granted(CurrentAccess),
    /// The run must stop here: why, as the user reads it (French).
    Ended(String),
}

/// Where a run reads its user's rights before each step (ADR-035): an account
/// disabled, or a right withdrawn, while the run goes on stops counting at
/// the next step instead of at the end of the run.
#[async_trait]
pub trait AccessSource: Send + Sync {
    async fn current(&self) -> anyhow::Result<StepAccess>;
}

#[derive(Clone)]
pub struct EnginePorts {
    pub events: Arc<dyn RunEventSink>,
    pub audit: Arc<dyn AuditSink>,
    pub tool_log: Arc<dyn ToolInvocationLog>,
    pub approvals: Arc<dyn ApprovalStore>,
    pub usage: Arc<dyn UsageSink>,
    pub memory: Arc<dyn MemoryStore>,
    pub skills: Arc<dyn SkillStore>,
    pub runs: Arc<dyn RunStore>,
    pub deliverables: Arc<dyn DeliverableService>,
}
