//! Memory and skill ports of a run, backed by PostgreSQL (see `db_ports.rs`).

use async_trait::async_trait;
use mimorya_agent::runtime::{
    MemoryHit, MemoryStore, NewMemory, SkillContent, SkillStore, SkillSummary,
};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::{MemoryReadFilter, MemoryStatus};
use mimorya_db::repositories::{memory, skills};
use uuid::Uuid;

use super::context::memory_hit;
use super::db_ports::DbPorts;
use crate::services::embeddings::UsageBy;

#[async_trait]
impl MemoryStore for DbPorts {
    async fn search(
        &self,
        filter: &MemoryReadFilter,
        query: &str,
        limit: u32,
        clearance: Sensitivity,
    ) -> anyhow::Result<Vec<MemoryHit>> {
        let by = UsageBy {
            user_id: Some(self.scope.user_id),
            run_id: Some(self.scope.run_id),
        };
        let tenant_id = self.scope.tenant_id;
        let meaning = self.embeddings.meaning_of(tenant_id, query, by).await;
        let meaning = meaning.as_ref().map(|m| m.as_meaning());
        let mut tx = self.tx().await?;
        let limit = i64::from(limit);
        let hits = memory::search_memory(&mut tx, filter, query, meaning, limit, clearance).await?;
        tx.commit().await?;
        Ok(hits.into_iter().filter_map(memory_hit).collect())
    }

    async fn create(&self, input: NewMemory) -> anyhow::Result<Uuid> {
        let mut tx = self.tx().await?;
        let id = memory::create_memory(
            &mut tx,
            memory::NewMemory {
                tenant_id: self.scope.tenant_id,
                scope: input.scope,
                owner_user_id: input.owner_user_id,
                org_unit_id: input.org_unit_id,
                kind: input.kind,
                title: input.title,
                content: input.content,
                sensitivity: input.sensitivity,
                status: input.status,
                source: input.source,
                created_by_user_id: Some(self.scope.user_id),
                created_by_run_id: Some(self.scope.run_id),
            },
        )
        .await?;
        tx.commit().await?;
        // A proposal waits for its approval.
        if input.status == MemoryStatus::Active {
            self.embeddings.wake();
        }
        Ok(id)
    }

    async fn mark_used(&self, ids: &[Uuid]) -> anyhow::Result<()> {
        let mut tx = self.tx().await?;
        memory::mark_memory_used(&mut tx, ids).await?;
        Ok(tx.commit().await?)
    }
}

#[async_trait]
impl SkillStore for DbPorts {
    async fn list_active(&self) -> anyhow::Result<Vec<SkillSummary>> {
        let mut tx = self.tx().await?;
        let rows = skills::list_active_skills(&mut tx).await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|s| SkillSummary {
                name: s.name,
                description: s.description,
            })
            .collect())
    }

    async fn get_active(&self, name: &str) -> anyhow::Result<Option<SkillContent>> {
        let mut tx = self.tx().await?;
        let row = skills::get_active_skill(&mut tx, name).await?;
        tx.commit().await?;
        Ok(row.map(|s| SkillContent {
            name: s.name,
            version: s.version,
            content: s.content,
        }))
    }
}
