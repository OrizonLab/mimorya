//! The tool by which the agent proposes to the skills' administrators a
//! procedure written with the person it serves (ADR-100): a new version of
//! an active skill, or a new skill. Its text leaves the conversation for
//! people who could not read it: each proposal waits for the person to
//! validate it as written, and reaches no run before an administrator makes
//! it active.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::skill_proposals::{ProposalOutcome, ProposalTarget, SkillProposal};
use mimorya_core::skill_rules::{NAME_PATTERN, content_problem, line_problem, normalized_content};
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::{Validate, ValidationError};

use super::skill_tools::skill_name;
use crate::runtime::ports::SkillStore;

pub const SKILL_PROPOSE: &str = "skill_propose";

pub struct SkillPropose {
    pub skills: Arc<dyn SkillStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct SkillProposeInput {
    /// The active skill a new version is proposed for, or the new skill's name.
    #[schemars(regex(pattern = NAME_PATTERN), length(min = 2, max = 64))]
    #[validate(custom(function = "skill_name"))]
    name: String,
    /// For a new skill only: what it covers, in one line.
    #[schemars(length(min = 10, max = 300))]
    #[validate(custom(function = "description_line"))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// The whole procedure, in Markdown.
    #[schemars(length(min = 1, max = 20000))]
    #[validate(length(min = 1, max = 20000), custom(function = "procedure"))]
    content: String,
    /// Why, in one line: what the conversation taught.
    #[schemars(length(min = 3, max = 300))]
    #[validate(custom(function = "note_line"))]
    rationale: String,
}

/// One visible line of `min` to 300 characters, its blanks around removed
/// as the administration removes them.
fn visible_line(text: &str, min: usize) -> Result<(), ValidationError> {
    let text = text.trim();
    let length = text.chars().count();
    if (min..=300).contains(&length) && line_problem("Texte", text).is_none() {
        Ok(())
    } else {
        Err(ValidationError::new("visible_line"))
    }
}

fn description_line(text: &str) -> Result<(), ValidationError> {
    visible_line(text, 10)
}

fn note_line(text: &str) -> Result<(), ValidationError> {
    visible_line(text, 3)
}

/// Visible text, lines and tabulations only, as a version's content.
fn procedure(content: &str) -> Result<(), ValidationError> {
    match content_problem(&normalized_content(content)) {
        None => Ok(()),
        Some(_) => Err(ValidationError::new("procedure")),
    }
}

#[async_trait]
impl TypedTool for SkillPropose {
    type Input = SkillProposeInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            SKILL_PROPOSE,
            "Proposition de compétence",
            "Propose aux administrateurs des compétences une procédure tirée de la conversation : la nouvelle version complète d’une compétence active (sans description), ou une nouvelle compétence (avec sa description). L’utilisateur valide d’abord le texte exact ; aucune autre conversation n’en tient compte avant qu’un administrateur l’active. Le texte reste général : aucune donnée personnelle, d’un client ou d’un tiers, aucun secret, identifiant ni montant.",
            ToolRisk::Draft,
        )
        // It reads what it improves: never a skill its run may not read.
        .requires(["skill:read", "skill:propose"])
        .always_requires_approval()
    }

    fn describe_action(&self, input: &SkillProposeInput) -> Option<String> {
        Some(match input.description {
            Some(_) => format!(
                "Proposer aux administrateurs des compétences la nouvelle compétence « {} »",
                input.name
            ),
            None => format!(
                "Proposer aux administrateurs des compétences une nouvelle version de « {} »",
                input.name
            ),
        })
    }

    async fn run(
        &self,
        input: SkillProposeInput,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let name = input.name.clone();
        let proposal = SkillProposal {
            name: input.name,
            description: input.description.map(|d| d.trim().to_owned()),
            content: normalized_content(&input.content),
            rationale: input.rationale.trim().to_owned(),
        };
        let target = match self.skills.propose(proposal).await? {
            ProposalOutcome::Refused(why) => return Ok(ToolResult::error(why)),
            ProposalOutcome::Proposed { target, .. } => target,
        };
        // An inactive skill's proposal reads as a new skill's: no version
        // number tells them apart.
        let summary = match target {
            ProposalTarget::NewVersion => format!("Nouvelle version de « {name} » proposée"),
            ProposalTarget::NewSkill | ProposalTarget::InactiveSkill => {
                format!("Compétence « {name} » proposée")
            }
        };
        Ok(ToolResult::ok(json!({
            "competence": name,
            "statut": "proposée aux administrateurs des compétences, qui l’activeront ou la rejetteront",
        }))
        .with_summary(summary))
    }
}
