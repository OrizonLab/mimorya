//! The memory of the in-memory ports: what a run may read (as the platform
//! filters it), found by shared words. For tests only.

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::{
    MemoryKind, MemoryReadFilter, MemoryScope, MemorySource, MemoryStatus, UnitFilter,
};

use uuid::Uuid;

use super::in_memory_ports::{InMemoryPorts, StoredMemory};
use crate::runtime::ports::{MemoryHit, MemoryStore, NewMemory};

fn is_visible(memory: &NewMemory, filter: &MemoryReadFilter) -> bool {
    match memory.scope {
        MemoryScope::User => memory.owner_user_id == Some(filter.owner_user_id),
        _ if !filter.shared => false,
        MemoryScope::OrgUnit => match &filter.org_units {
            UnitFilter::All => true,
            UnitFilter::Only(units) => memory.org_unit_id.is_some_and(|u| {
                units.contains(&u)
                    // Shared with the reader's units, up to its sensitivity.
                    || (filter.from_shares.iter())
                        .any(|&(unit, cap)| unit == u && memory.sensitivity <= cap)
            }),
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

pub(super) fn tokenize(text: &str) -> Vec<String> {
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
