//! Built-in skill tool: loads the full content of an active company skill,
//! and the quality criteria its answer is reviewed on (ADR-106), told to the
//! agent with the procedure so that it meets them from its first answer.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::skill_rules::{NAME_PATTERN, name_problem};
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use validator::{Validate, ValidationError};

use crate::runtime::ports::SkillStore;

/// The tool that loads a skill: its result reaches the model as a procedure
/// (ADR-052).
pub const SKILL_READ: &str = "skill_read";

pub struct SkillRead {
    pub skills: Arc<dyn SkillStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct SkillReadInput {
    #[schemars(regex(pattern = NAME_PATTERN), length(min = 2, max = 64))]
    #[validate(custom(function = "skill_name"))]
    name: String,
}

/// A name as the administration writes it (`skill_rules`).
pub(super) fn skill_name(name: &str) -> Result<(), ValidationError> {
    match name_problem(name) {
        None => Ok(()),
        Some(_) => Err(ValidationError::new("skill_name")),
    }
}

#[async_trait]
impl TypedTool for SkillRead {
    type Input = SkillReadInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            SKILL_READ,
            "Lecture d’une compétence",
            "Charge le contenu complet d’une compétence (procédure validée par l’entreprise) listée dans les consignes. À utiliser avant d’exécuter une tâche couverte par une compétence.",
            ToolRisk::Read,
        )
        .requires(["skill:read"])
    }

    async fn run(&self, input: SkillReadInput, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let Some(skill) = self.skills.get_active(&input.name).await? else {
            return Ok(ToolResult::error(format!(
                "Compétence « {} » introuvable",
                input.name
            )));
        };
        let content = with_criteria(skill.content, &skill.quality_criteria);
        Ok(ToolResult::ok(Value::String(content))
            .with_summary(format!("Compétence {} v{}", skill.name, skill.version))
            .with_quality_criteria(skill.quality_criteria)
            .with_citations(vec![Citation::new(
                "skill",
                format!("{}@{}", skill.name, skill.version),
            )]))
    }
}

/// The procedure, then what its answer is reviewed on, when anything is.
fn with_criteria(content: String, criteria: &[String]) -> String {
    if criteria.is_empty() {
        return content;
    }
    let list: Vec<String> = criteria.iter().map(|c| format!("- {c}")).collect();
    format!(
        "{content}\n\n## Critères de qualité de la réponse\n{}",
        list.join("\n")
    )
}
