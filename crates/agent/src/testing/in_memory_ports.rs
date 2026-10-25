//! In-memory implementations of the engine ports, for tests and local
//! experiments. Not for production use.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mimorya_core::tools::{Deliverable, DeliverableInput, DeliverableService, DeliverableStatus};
use uuid::Uuid;

use super::in_memory_documents::DocumentFixture;
use crate::runtime::ports::{
    ApprovalPlacement, ApprovalRequest, ApprovalStore, AuditSink, EngineAuditEntry, EnginePorts,
    NewMemory, RunEvent, RunEventSink, RunStore, SkillContent, SkillStore, SkillSummary,
    ToolInvocationLog, ToolInvocationRecord, UsageRecord, UsageSink,
};
use crate::runtime::{RunCheckpoint, RunOutcome};

#[derive(Debug, Clone, PartialEq)]
pub struct StoredMemory {
    pub id: Uuid,
    pub memory: NewMemory,
    pub use_count: u32,
}

#[derive(Debug, Clone)]
pub struct SkillFixture {
    pub name: String,
    pub description: String,
    pub content: String,
}

/// A recorded outcome, with the number of events emitted before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinishedRecord {
    pub status: &'static str,
    pub events_before: usize,
}

#[derive(Default)]
pub(super) struct State {
    emitted: Vec<RunEvent>,
    deltas: Vec<String>,
    audit: Vec<EngineAuditEntry>,
    tool_records: Vec<ToolInvocationRecord>,
    approvals: Vec<(Uuid, ApprovalRequest)>,
    usage: Vec<UsageRecord>,
    pub(super) memories: Vec<StoredMemory>,
    pub(super) documents: Vec<DocumentFixture>,
    checkpoints: Vec<RunCheckpoint>,
    finished: Vec<FinishedRecord>,
    deliverables: HashMap<Uuid, Deliverable>,
    /// Set, requests for validation are refused for this reason (ADR-059).
    unroutable: Option<String>,
}

pub struct InMemoryPorts {
    user_id: Uuid,
    run_id: Uuid,
    skills: Vec<SkillFixture>,
    state: Mutex<State>,
}

impl InMemoryPorts {
    pub fn new(user_id: Uuid, run_id: Uuid) -> Arc<Self> {
        Self::with_fixtures(user_id, run_id, Vec::new(), Vec::new())
    }

    pub fn with_fixtures(
        user_id: Uuid,
        run_id: Uuid,
        memories: Vec<NewMemory>,
        skills: Vec<SkillFixture>,
    ) -> Arc<Self> {
        let state = State {
            memories: memories
                .into_iter()
                .map(|memory| StoredMemory {
                    id: Uuid::new_v4(),
                    memory,
                    use_count: 0,
                })
                .collect(),
            ..State::default()
        };
        Arc::new(Self {
            user_id,
            run_id,
            skills,
            state: Mutex::new(state),
        })
    }

    /// All ports backed by this store.
    pub fn ports(self: &Arc<Self>) -> EnginePorts {
        EnginePorts {
            events: self.clone(),
            audit: self.clone(),
            tool_log: self.clone(),
            approvals: self.clone(),
            usage: self.clone(),
            memory: self.clone(),
            documents: self.clone(),
            skills: self.clone(),
            runs: self.clone(),
            deliverables: self.clone(),
        }
    }

    /// Deposits `document`, for the runs to read where their filter reaches.
    pub fn deposit(&self, document: DocumentFixture) {
        self.state().documents.push(document);
    }

    pub(super) fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("in-memory ports")
    }

    pub fn emitted(&self) -> Vec<RunEvent> {
        self.state().emitted.clone()
    }

    pub fn deltas(&self) -> Vec<String> {
        self.state().deltas.clone()
    }

    pub fn audit_entries(&self) -> Vec<EngineAuditEntry> {
        self.state().audit.clone()
    }

    pub fn tool_records(&self) -> Vec<ToolInvocationRecord> {
        self.state().tool_records.clone()
    }

    /// From now on, nobody may validate a request: each is refused for `why`.
    pub fn nobody_validates(&self, why: &str) {
        self.state().unroutable = Some(why.into());
    }

    pub fn approval_requests(&self) -> Vec<(Uuid, ApprovalRequest)> {
        self.state().approvals.clone()
    }

    pub fn usage_records(&self) -> Vec<UsageRecord> {
        self.state().usage.clone()
    }

    pub fn memories(&self) -> Vec<StoredMemory> {
        self.state().memories.clone()
    }

    pub fn checkpoints(&self) -> Vec<RunCheckpoint> {
        self.state().checkpoints.clone()
    }

    pub fn finished(&self) -> Vec<FinishedRecord> {
        self.state().finished.clone()
    }

    pub fn deliverable(&self, id: Uuid) -> Option<Deliverable> {
        self.state().deliverables.get(&id).cloned()
    }
}

#[async_trait]
impl RunEventSink for InMemoryPorts {
    async fn emit(&self, event: RunEvent) -> anyhow::Result<()> {
        self.state().emitted.push(event);
        Ok(())
    }

    fn publish_delta(&self, text: &str) {
        self.state().deltas.push(text.into());
    }
}

#[async_trait]
impl AuditSink for InMemoryPorts {
    async fn record(&self, entry: EngineAuditEntry) -> anyhow::Result<()> {
        self.state().audit.push(entry);
        Ok(())
    }
}

#[async_trait]
impl ToolInvocationLog for InMemoryPorts {
    async fn record(&self, entry: ToolInvocationRecord) -> anyhow::Result<()> {
        self.state().tool_records.push(entry);
        Ok(())
    }
}

#[async_trait]
impl ApprovalStore for InMemoryPorts {
    async fn create(&self, request: ApprovalRequest) -> anyhow::Result<ApprovalPlacement> {
        let mut state = self.state();
        if let Some(why) = state.unroutable.clone() {
            return Ok(ApprovalPlacement::Unroutable(why));
        }
        let id = Uuid::new_v4();
        state.approvals.push((id, request));
        Ok(ApprovalPlacement::Pending(id))
    }
}

#[async_trait]
impl UsageSink for InMemoryPorts {
    async fn record(&self, usage: UsageRecord) -> anyhow::Result<()> {
        self.state().usage.push(usage);
        Ok(())
    }
}

#[async_trait]
impl SkillStore for InMemoryPorts {
    async fn list_active(&self) -> anyhow::Result<Vec<SkillSummary>> {
        Ok(self
            .skills
            .iter()
            .map(|s| SkillSummary {
                name: s.name.clone(),
                description: s.description.clone(),
            })
            .collect())
    }

    async fn get_active(&self, name: &str) -> anyhow::Result<Option<SkillContent>> {
        Ok(self
            .skills
            .iter()
            .find(|s| s.name == name)
            .map(|s| SkillContent {
                name: s.name.clone(),
                version: 1,
                content: s.content.clone(),
            }))
    }
}

#[async_trait]
impl RunStore for InMemoryPorts {
    async fn checkpoint(&self, state: &RunCheckpoint) -> anyhow::Result<()> {
        self.state().checkpoints.push(state.clone());
        Ok(())
    }

    async fn finish(&self, outcome: &RunOutcome) -> anyhow::Result<()> {
        let mut state = self.state();
        state.checkpoints.push(outcome.checkpoint().clone());
        let events_before = state.emitted.len();
        state.finished.push(FinishedRecord {
            status: outcome.status(),
            events_before,
        });
        Ok(())
    }
}

#[async_trait]
impl DeliverableService for InMemoryPorts {
    async fn create(&self, input: DeliverableInput) -> anyhow::Result<Deliverable> {
        let deliverable = Deliverable {
            id: Uuid::new_v4(),
            kind: input.kind,
            title: input.title,
            content: input.content,
            org_unit_id: input.org_unit_id,
            owner_user_id: self.user_id,
            run_id: Some(self.run_id),
            status: DeliverableStatus::Draft,
        };
        self.state()
            .deliverables
            .insert(deliverable.id, deliverable.clone());
        Ok(deliverable)
    }

    async fn get(&self, id: Uuid) -> anyhow::Result<Option<Deliverable>> {
        Ok(self.state().deliverables.get(&id).cloned())
    }

    async fn set_status(&self, id: Uuid, status: DeliverableStatus) -> anyhow::Result<()> {
        if let Some(deliverable) = self.state().deliverables.get_mut(&id) {
            deliverable.status = status;
        }
        Ok(())
    }
}
