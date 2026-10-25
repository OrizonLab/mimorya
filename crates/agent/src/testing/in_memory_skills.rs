//! The skills of the in-memory ports: the active ones a run reads, each at
//! its first version, and the proposals the agent makes of them (ADR-100),
//! judged by the platform's rules. For tests only.

use async_trait::async_trait;
use mimorya_core::skill_proposals::{
    NamedSkill, ProposalOutcome, ProposalTarget, SkillProposal, proposal_target,
};

use super::in_memory_ports::InMemoryPorts;
use crate::runtime::{SkillContent, SkillStore, SkillSummary};

/// An active skill of the company, at its first version.
#[derive(Debug, Clone)]
pub struct SkillFixture {
    pub name: String,
    pub description: String,
    pub content: String,
}

impl InMemoryPorts {
    /// The skills proposed, in order.
    pub fn skill_proposals(&self) -> Vec<SkillProposal> {
        self.state().skill_proposals.clone()
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

    async fn propose(&self, proposal: SkillProposal) -> anyhow::Result<ProposalOutcome> {
        let mut state = self.state();
        let active = self.skills.iter().any(|s| s.name == proposal.name);
        let proposed = state
            .skill_proposals
            .iter()
            .filter(|p| p.name == proposal.name)
            .count();
        // A new skill proposed already is a draft: no longer free.
        let named = match (active, proposed) {
            (true, _) => NamedSkill::Active,
            (false, 0) => NamedSkill::Unknown,
            (false, _) => NamedSkill::Inactive,
        };
        let pending = state.skill_proposals.len() as i64;
        Ok(match proposal_target(&proposal, named, pending) {
            Err(why) => ProposalOutcome::Refused(why),
            Ok(target) => {
                let version = match target {
                    // After the active version 1 and those proposed before.
                    ProposalTarget::NewVersion => 2 + proposed as i32,
                    ProposalTarget::NewSkill => 1,
                };
                state.skill_proposals.push(proposal);
                ProposalOutcome::Proposed { target, version }
            }
        })
    }
}
