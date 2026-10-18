//! Built-in skill tool: loads the full content of an active company skill.

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use validator::Validate;

use crate::runtime::ports::SkillStore;

static SKILL_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").expect("valid regex"));

pub struct SkillRead {
    pub skills: Arc<dyn SkillStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct SkillReadInput {
    #[schemars(regex(pattern = r"^[a-z0-9]+(-[a-z0-9]+)*$"))]
    #[validate(regex(path = *SKILL_NAME_RE))]
    name: String,
}

#[async_trait]
impl TypedTool for SkillRead {
    type Input = SkillReadInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "skill_read",
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
        Ok(ToolResult::ok(Value::String(skill.content))
            .with_summary(format!("Compétence {} v{}", skill.name, skill.version))
            .with_citations(vec![Citation::new(
                "skill",
                format!("{}@{}", skill.name, skill.version),
            )]))
    }
}
