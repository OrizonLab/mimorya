//! Tool contract shared by the agent engine and connectors. A tool never
//! decides authorization on its own: the engine's policy enforcement point
//! checks `required_permissions` and autonomy before `execute`, and the tool
//! receives a bound authorization view to filter the data it returns.

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::authz::{Actor, AuthorizationEngine, DataScope, Decision, ResourceRef, Subject};
use crate::autonomy::ToolRisk;
use crate::documents::{DocumentReadFilter, document_read_filter};
use crate::memory::{MemoryReadFilter, MemoryTarget, can_write_memory, memory_read_filter};

/// LLM providers accept `^[a-zA-Z0-9_-]{1,64}$`; we use `<namespace>_<action>`.
static TOOL_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]{1,63}$").expect("valid regex"));

pub fn is_valid_tool_name(name: &str) -> bool {
    TOOL_NAME_RE.is_match(name)
}

/// Static description of a tool. Administrators may override the permissions,
/// risk and approval requirement per tenant (see the agent's registry).
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    /// Short French label displayed to users (activity feed, approvals).
    pub title: String,
    /// Instructions for the model.
    pub description: String,
    pub risk: ToolRisk,
    /// All of these must be held (in some scope) before the tool may run.
    pub required_permissions: Vec<String>,
    pub requires_approval: bool,
    /// Identifiers matching this pattern in an answer must come from this
    /// tool's citations (grounding check), e.g. `\bINT-\d{4}\b`.
    pub citation_pattern: Option<Regex>,
}

impl ToolInfo {
    /// Panics on an invalid name: tool names are static in code.
    pub fn new(name: &str, title: &str, description: &str, risk: ToolRisk) -> Self {
        Self::try_new(name, title, description, risk).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Fallible constructor for tools described at runtime (MCP servers).
    pub fn try_new(
        name: &str,
        title: &str,
        description: &str,
        risk: ToolRisk,
    ) -> Result<Self, String> {
        if !is_valid_tool_name(name) {
            return Err(format!(
                "Invalid tool name \"{name}\" (expected {})",
                TOOL_NAME_RE.as_str()
            ));
        }
        Ok(Self {
            name: name.into(),
            title: title.into(),
            description: description.into(),
            risk,
            required_permissions: Vec::new(),
            requires_approval: false,
            citation_pattern: None,
        })
    }

    pub fn requires<I: IntoIterator<Item = S>, S: Into<String>>(mut self, permissions: I) -> Self {
        self.required_permissions = permissions.into_iter().map(Into::into).collect();
        self
    }

    pub fn always_requires_approval(mut self) -> Self {
        self.requires_approval = true;
        self
    }

    /// Panics on an invalid pattern: citation patterns are static in code.
    pub fn citing(mut self, pattern: &str) -> Self {
        self.citation_pattern = Some(Regex::new(pattern).expect("valid citation pattern"));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Citation {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Citation {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            label: None,
        }
    }

    pub fn labelled(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    /// JSON payload given to the model (as untrusted data).
    pub content: Value,
    /// Short human summary for activity feeds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub is_error: bool,
    /// Identifiers of the records the result is built from (grounding, citations).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<Citation>,
    /// The quality criteria of the skill version loaded (ADR-106): the
    /// answer is reviewed on them. Honoured from the platform's
    /// `skill_read` alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quality_criteria: Vec<String>,
}

impl ToolResult {
    pub fn ok(content: Value) -> Self {
        Self {
            content,
            ..Self::default()
        }
    }

    /// An error reported to the model, in French.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: json!({ "erreur": message.into() }),
            is_error: true,
            ..Self::default()
        }
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn with_citations(mut self, citations: Vec<Citation>) -> Self {
        self.citations = citations;
        self
    }

    pub fn with_quality_criteria(mut self, criteria: Vec<String>) -> Self {
        self.quality_criteria = criteria;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableStatus {
    Draft,
    Approved,
    Applied,
    Discarded,
}

impl DeliverableStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Applied => "applied",
            Self::Discarded => "discarded",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Draft, Self::Approved, Self::Applied, Self::Discarded]
            .into_iter()
            .find(|s| s.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliverableInput {
    pub kind: String,
    pub title: String,
    pub content: Value,
    pub org_unit_id: Option<Uuid>,
}

/// Something the agent produced (draft, report, proposal…).
#[derive(Debug, Clone, PartialEq)]
pub struct Deliverable {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub content: Value,
    pub org_unit_id: Option<Uuid>,
    pub owner_user_id: Uuid,
    pub run_id: Option<Uuid>,
    pub status: DeliverableStatus,
}

/// Platform service letting tools record what the agent produces.
#[async_trait]
pub trait DeliverableService: Send + Sync {
    async fn create(&self, input: DeliverableInput) -> anyhow::Result<Deliverable>;
    async fn get(&self, id: Uuid) -> anyhow::Result<Option<Deliverable>>;
    async fn set_status(&self, id: Uuid, status: DeliverableStatus) -> anyhow::Result<()>;
}

/// The authorization engine bound to the subject and actor of a run.
#[derive(Clone)]
pub struct BoundAuthorization {
    engine: Arc<AuthorizationEngine>,
    subject: Arc<Subject>,
    actor: Actor,
}

impl BoundAuthorization {
    pub fn new(engine: Arc<AuthorizationEngine>, subject: Arc<Subject>, actor: Actor) -> Self {
        Self {
            engine,
            subject,
            actor,
        }
    }

    /// The memories the subject may read through this actor.
    pub fn memory_read_filter(&self) -> MemoryReadFilter {
        memory_read_filter(&self.engine, &self.subject, &self.actor)
    }

    /// The documents the subject may read through this actor (ADR-095).
    pub fn document_read_filter(&self) -> DocumentReadFilter {
        document_read_filter(&self.engine, &self.subject, &self.actor)
    }

    /// Whether the subject may write `target` directly through this actor.
    pub fn can_write_memory(&self, target: MemoryTarget) -> bool {
        can_write_memory(&self.engine, &self.subject, target, &self.actor)
    }

    pub fn scope(&self, permission: &str) -> DataScope {
        self.engine
            .data_scope(&self.subject, permission, &self.actor)
    }

    /// Checks a resource of the subject's tenant (its `tenant_id` is overwritten).
    pub fn check(&self, permission: &str, mut resource: ResourceRef) -> Decision {
        resource.tenant_id = self.subject.tenant_id;
        self.engine
            .check(&self.subject, permission, &resource, &self.actor)
    }
}

pub struct ToolContext {
    pub tenant_id: Uuid,
    pub subject: Arc<Subject>,
    pub actor: Actor,
    pub run_id: Uuid,
    pub authz: BoundAuthorization,
    pub deliverables: Arc<dyn DeliverableService>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn info(&self) -> &ToolInfo;

    /// JSON Schema of the input, sent to the model.
    fn input_schema(&self) -> Value;

    /// Validates the model's input. Returns the normalised input, or the
    /// issues in French (they are reported to the model).
    fn parse_input(&self, raw: &Value) -> Result<Value, Vec<String>>;

    /// Human readable description of a pending action, shown on approval requests.
    fn describe_action(&self, _input: &Value) -> Option<String> {
        None
    }

    /// Runs the tool with an input returned by `parse_input`. An error is
    /// reported to the model as a failed call.
    async fn execute(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
}

pub type ToolRef = Arc<dyn Tool>;
