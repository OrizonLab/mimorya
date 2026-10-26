//! What a case played for an evaluation reads (ADR-104): the memory and the
//! documents in reach of whom it runs for, as a run reads them, without
//! writing (nothing marked as used); and the company's skills, the version
//! evaluated in place of its skill's active one, listed in the index even
//! when its skill is not active yet. Whether the case loaded it is told
//! with its result.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mimorya_agent::runtime::{
    DocumentExcerpt, DocumentHit, DocumentStore, MemoryHit, MemoryStore, NewMemory, SkillContent,
    SkillStore, SkillSummary,
};
use mimorya_core::authz::Sensitivity;
use mimorya_core::documents::DocumentReadFilter;
use mimorya_core::memory::MemoryReadFilter;
use mimorya_core::skill_proposals::{ProposalOutcome, SkillProposal};
use mimorya_db::Database;
use mimorya_db::repositories::skills::{get_active_skill, list_active_skills};
use uuid::Uuid;

use super::CaseScope;
use crate::services::embeddings::{EmbeddingService, UsageBy};
use crate::services::runs::Reader;

/// Why a case does not propose a skill (never offered: a draft).
const NO_PROPOSAL: &str = "Une évaluation ne propose pas de compétence.";

/// The version a case is played with, as the index and `skill_read` give it.
#[derive(Debug, Clone)]
pub struct EvaluatedVersion {
    pub name: String,
    pub description: String,
    pub version: i32,
    pub content: String,
    /// What an answer that follows it must meet (ADR-106).
    pub quality_criteria: Vec<String>,
}

pub struct Knowledge {
    db: Database,
    embeddings: Arc<EmbeddingService>,
    scope: CaseScope,
    evaluated: EvaluatedVersion,
    loaded: AtomicBool,
}

impl Knowledge {
    pub fn new(
        db: Database,
        embeddings: Arc<EmbeddingService>,
        scope: CaseScope,
        evaluated: EvaluatedVersion,
    ) -> Self {
        Self {
            db,
            embeddings,
            scope,
            evaluated,
            loaded: AtomicBool::new(false),
        }
    }

    /// Whether the case loaded the version evaluated.
    pub fn loaded(&self) -> bool {
        self.loaded.load(Ordering::Relaxed)
    }

    fn reader(&self) -> Reader<'_> {
        Reader {
            db: &self.db,
            embeddings: &self.embeddings,
            tenant_id: self.scope.tenant_id,
            by: UsageBy {
                user_id: Some(self.scope.user_id),
                run_id: None,
            },
        }
    }
}

#[async_trait]
impl MemoryStore for Knowledge {
    async fn search(
        &self,
        filter: &MemoryReadFilter,
        query: &str,
        limit: u32,
        clearance: Sensitivity,
    ) -> anyhow::Result<Vec<MemoryHit>> {
        let reader = self.reader();
        reader.search_memory(filter, query, limit, clearance).await
    }

    async fn create(&self, _memory: NewMemory) -> anyhow::Result<Uuid> {
        anyhow::bail!("an evaluation writes no memory")
    }

    /// An evaluation's reads say nothing of what people use.
    async fn mark_used(&self, _ids: &[Uuid]) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait]
impl DocumentStore for Knowledge {
    async fn search(
        &self,
        filter: &DocumentReadFilter,
        query: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<DocumentHit>> {
        self.reader().search_documents(filter, query, limit).await
    }

    async fn read(
        &self,
        filter: &DocumentReadFilter,
        document_id: Uuid,
        from: u32,
        count: u32,
    ) -> anyhow::Result<Option<DocumentExcerpt>> {
        let reader = self.reader();
        reader.read_document(filter, document_id, from, count).await
    }
}

#[async_trait]
impl SkillStore for Knowledge {
    async fn list_active(&self) -> anyhow::Result<Vec<SkillSummary>> {
        let mut tx = self.db.begin(self.scope.tenant_id).await?;
        let rows = list_active_skills(&mut tx).await?;
        tx.commit().await?;
        let evaluated = &self.evaluated;
        let mut skills: Vec<SkillSummary> = rows
            .into_iter()
            .filter(|s| s.name != evaluated.name)
            .map(|s| SkillSummary {
                name: s.name,
                description: s.description,
            })
            .collect();
        skills.push(SkillSummary {
            name: evaluated.name.clone(),
            description: evaluated.description.clone(),
        });
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }

    async fn get_active(&self, name: &str) -> anyhow::Result<Option<SkillContent>> {
        let evaluated = &self.evaluated;
        if name == evaluated.name {
            self.loaded.store(true, Ordering::Relaxed);
            return Ok(Some(SkillContent {
                name: evaluated.name.clone(),
                version: evaluated.version,
                content: evaluated.content.clone(),
                quality_criteria: evaluated.quality_criteria.clone(),
            }));
        }
        let mut tx = self.db.begin(self.scope.tenant_id).await?;
        let row = get_active_skill(&mut tx, name).await?;
        tx.commit().await?;
        Ok(row.map(|s| SkillContent {
            name: s.name,
            version: s.version,
            content: s.content,
            quality_criteria: s.quality_criteria,
        }))
    }

    async fn propose(&self, _proposal: SkillProposal) -> anyhow::Result<ProposalOutcome> {
        Ok(ProposalOutcome::Refused(NO_PROPOSAL.into()))
    }
}
