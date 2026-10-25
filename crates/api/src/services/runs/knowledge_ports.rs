//! Memory and skill ports of a run, backed by PostgreSQL (see `db_ports.rs`);
//! the skills it proposes too (ADR-100).

use async_trait::async_trait;
use mimorya_agent::runtime::{
    MemoryHit, MemoryStore, NewMemory, SkillContent, SkillStore, SkillSummary,
};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::{MemoryReadFilter, MemoryStatus};
use mimorya_core::skill_proposals::{
    NamedSkill, ProposalOutcome, ProposalTarget, SkillProposal, proposal_target,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::skills::{NewSkill, NewSkillVersion};
use mimorya_db::repositories::{memory, skill_proposals, skills};
use serde_json::json;
use uuid::Uuid;

use super::context::memory_hit;
use super::db_ports::{DbPorts, RunScope};
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
        let mut tx = self.held_tx().await?;
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

    async fn propose(&self, proposal: SkillProposal) -> anyhow::Result<ProposalOutcome> {
        let mut tx = self.held_tx().await?;
        // Beside the administration's changes of the skills (ADR-020).
        tx.lock_administration().await?;
        let named = skill_proposals::skill_named(&mut tx, &proposal.name).await?;
        let state = match named.as_ref().map(|s| s.status.as_str()) {
            None => NamedSkill::Unknown,
            Some("active") => NamedSkill::Active,
            Some(_) => NamedSkill::Inactive,
        };
        let pending = skill_proposals::count_pending_proposals(&mut tx).await?;
        let target = match proposal_target(&proposal, state, pending) {
            Ok(target) => target,
            Err(why) => return Ok(ProposalOutcome::Refused(why)),
        };
        let RunScope {
            tenant_id,
            run_id,
            user_id,
            ..
        } = self.scope;
        let skill_id = match (target, named, &proposal.description) {
            (ProposalTarget::NewVersion, Some(skill), _) => skill.id,
            (_, _, Some(description)) => {
                let skill = NewSkill {
                    tenant_id,
                    name: &proposal.name,
                    description,
                    created_by: user_id,
                };
                skills::insert_skill(&mut tx, skill).await?
            }
            _ => anyhow::bail!("a proposal's target without its skill"),
        };
        let version = NewSkillVersion {
            tenant_id,
            skill_id,
            content: &proposal.content,
            changelog: &proposal.rationale,
            created_by: user_id,
            proposed_by_run: Some(run_id),
        };
        let version = skills::insert_skill_version(&mut tx, version).await?;
        // Its text stays out of the audit: the version keeps it.
        let proposed = AuditEntry::new(tenant_id, "agent", "skill.proposed", "success")
            .on_behalf_of(user_id)
            .run(run_id)
            .resource("skill", skill_id)
            .metadata(json!({
                "name": proposal.name,
                "version": version,
                "newSkill": target == ProposalTarget::NewSkill,
            }));
        append_audit(&mut tx, proposed).await?;
        tx.commit().await?;
        Ok(ProposalOutcome::Proposed { target, version })
    }
}
