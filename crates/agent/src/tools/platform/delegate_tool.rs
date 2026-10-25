//! The tool that hands a precise task to a sub-agent (ADR-098). The policy
//! enforcement point judges its calls like any other's; the engine, not the
//! tool, then runs each allowed one as a child run (`runtime/delegation`).

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::runtime::SubAgentProfile;

pub const DELEGATE_TASK: &str = "delegate_task";

/// The longest task, and the longest context passed with it, in characters.
const MAX_TASK: u64 = 2_000;
const MAX_CONTEXT: u64 = 8_000;

pub struct DelegateTask;

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct DelegateTaskInput {
    /// `research`, `analysis`, `planning` or `quality`.
    pub profile: SubAgentProfile,
    /// The task, precise and self-contained.
    #[schemars(length(min = 1, max = 2000))]
    #[validate(length(min = 1, max = 2000))]
    pub task: String,
    /// What the sub-agent needs to know that it cannot find itself.
    #[schemars(length(max = 8000))]
    #[validate(length(max = 8000))]
    pub context: Option<String>,
}

#[async_trait]
impl TypedTool for DelegateTask {
    type Input = DelegateTaskInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            DELEGATE_TASK,
            "Délégation à un sous-agent",
            &format!(
                "Confie une tâche précise et autonome à un sous-agent spécialisé, qui rend un résumé avec ses sources. Profils : « research » (rassembler les faits et leurs sources), « analysis » (constats chiffrés sur des données), « planning » (plan ou découpage en étapes), « quality » (relecture d’un contenu au regard de critères). Le sous-agent ne fait que lire, avec tes droits au plus, sur une part de ton budget ; au plus trois par étape, exécutés en parallèle. La tâche tient en {MAX_TASK} caractères, le contexte transmis en {MAX_CONTEXT}."
            ),
            ToolRisk::Read,
        )
    }

    async fn run(
        &self,
        _input: DelegateTaskInput,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        // The engine runs allowed calls itself; nothing else may.
        Ok(ToolResult::error(
            "Une délégation n’est exécutée que par le moteur de l’agent.",
        ))
    }
}
