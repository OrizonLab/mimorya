//! In-memory implementations of the engine ports, for tests and local
//! experiments. Not for production use.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::{
    MemoryKind, MemoryReadFilter, MemoryScope, MemorySource, MemoryStatus, UnitFilter,
};
use mimorya_core::tools::{Deliverable, DeliverableInput, DeliverableService, DeliverableStatus};
use uuid::Uuid;

use crate::runtime::ports::{
    ApprovalRequest, ApprovalStore, AuditSink, EngineAuditEntry, EnginePorts, MemoryHit,
    MemoryStore, NewMemory, RunEvent, RunEventSink, RunStore, SkillContent, SkillStore,
    SkillSummary, ToolInvocationLog, ToolInvocationRecord, UsageRecord, UsageSink,
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
struct State {
    emitted: Vec<RunEvent>,
    deltas: Vec<String>,
    audit: Vec<EngineAuditEntry>,
    tool_records: Vec<ToolInvocationRecord>,
    approvals: Vec<(Uuid, ApprovalRequest)>,
    usage: Vec<UsageRecord>,
    memories: Vec<StoredMemory>,
    checkpoints: Vec<RunCheckpoint>,
    finished: Vec<FinishedRecord>,
    deliverables: HashMap<Uuid, Deliverable>,
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
            skills: self.clone(),
            runs: self.clone(),
            deliverables: self.clone(),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
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
    async fn create(&self, request: ApprovalRequest) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        self.state().approvals.push((id, request));
        Ok(id)
    }
}

#[async_trait]
impl UsageSink for InMemoryPorts {
    async fn record(&self, usage: UsageRecord) -> anyhow::Result<()> {
        self.state().usage.push(usage);
        Ok(())
    }
}

fn is_visible(memory: &NewMemory, filter: &MemoryReadFilter) -> bool {
    match memory.scope {
        MemoryScope::User => memory.owner_user_id == Some(filter.owner_user_id),
        _ if !filter.shared => false,
        MemoryScope::OrgUnit => match &filter.org_units {
            UnitFilter::All => true,
            UnitFilter::Only(units) => memory.org_unit_id.is_some_and(|u| units.contains(&u)),
        },
        MemoryScope::Tenant | MemoryScope::Experience => true,
    }
}

fn fold(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        other => other,
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(fold)
        .collect::<String>()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() > 2)
        .map(str::to_string)
        .collect()
}

#[async_trait]
impl MemoryStore for InMemoryPorts {
    async fn search(
        &self,
        filter: &MemoryReadFilter,
        query: &str,
        limit: u32,
        clearance: Sensitivity,
    ) -> anyhow::Result<Vec<MemoryHit>> {
        let terms = tokenize(query);
        let state = self.state();
        let mut hits: Vec<MemoryHit> = state
            .memories
            .iter()
            .filter(|m| m.memory.status == MemoryStatus::Active && is_visible(&m.memory, filter))
            .filter(|m| {
                m.memory.owner_user_id == Some(filter.owner_user_id)
                    || m.memory.sensitivity <= clearance
            })
            .filter_map(|m| {
                let words = tokenize(&format!("{} {}", m.memory.title, m.memory.content));
                let score = terms.iter().filter(|t| words.contains(t)).count();
                (score > 0).then(|| MemoryHit {
                    id: m.id,
                    scope: m.memory.scope,
                    kind: m.memory.kind,
                    title: m.memory.title.clone(),
                    content: m.memory.content.clone(),
                    org_unit_id: m.memory.org_unit_id,
                    updated_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                    score: score as f64,
                })
            })
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(limit as usize);
        Ok(hits)
    }

    async fn create(&self, memory: NewMemory) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        self.state().memories.push(StoredMemory {
            id,
            memory,
            use_count: 0,
        });
        Ok(id)
    }

    async fn mark_used(&self, ids: &[Uuid]) -> anyhow::Result<()> {
        for memory in self
            .state()
            .memories
            .iter_mut()
            .filter(|m| ids.contains(&m.id))
        {
            memory.use_count += 1;
        }
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

/// A memory fixture with sensible defaults.
pub fn memory_fixture(
    scope: MemoryScope,
    kind: MemoryKind,
    title: &str,
    content: &str,
) -> NewMemory {
    NewMemory {
        scope,
        owner_user_id: None,
        org_unit_id: None,
        kind,
        title: title.into(),
        content: content.into(),
        sensitivity: Sensitivity::Internal,
        status: MemoryStatus::Active,
        source: MemorySource::manual(None),
    }
}
