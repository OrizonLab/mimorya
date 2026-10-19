//! Built-in memory tools: search the memories the user may read, propose new ones.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::memory::{
    MemoryKind, MemoryScope, MemorySource, MemoryStatus, MemoryTarget, can_write_memory,
    memory_read_filter,
};
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use crate::runtime::ports::{MemoryStore, NewMemory};

const DEFAULT_SEARCH_LIMIT: u32 = 5;

pub struct MemorySearch {
    pub memory: Arc<dyn MemoryStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MemorySearchInput {
    /// Termes recherchés
    #[validate(length(min = 2, max = 300))]
    query: String,
    #[validate(range(min = 1, max = 10))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

#[async_trait]
impl TypedTool for MemorySearch {
    type Input = MemorySearchInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "memory_search",
            "Recherche dans la mémoire",
            "Recherche dans les mémoires accessibles à l’utilisateur (personnelle, équipe, entreprise, expérience) : procédures, préférences, décisions, faits utiles.",
            ToolRisk::Read,
        )
        .requires(["agent:use"])
    }

    async fn run(&self, input: MemorySearchInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let filter = memory_read_filter(ctx.authz.engine(), &ctx.subject, &ctx.actor);
        let limit = input.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        let hits = self
            .memory
            .search(&filter, &input.query, limit, ctx.subject.clearance())
            .await?;
        let ids: Vec<_> = hits.iter().map(|h| h.id).collect();
        self.memory.mark_used(&ids).await?;
        let content: Vec<_> = hits
            .iter()
            .map(|h| {
                json!({
                    "id": h.id,
                    "espace": h.scope,
                    "type": h.kind,
                    "titre": h.title,
                    "contenu": h.content,
                    "maj": h.updated_at.format("%Y-%m-%d").to_string(),
                })
            })
            .collect();
        Ok(ToolResult::ok(json!(content))
            .with_summary(format!("{} élément(s) de mémoire trouvé(s)", hits.len()))
            .with_citations(
                hits.iter()
                    .map(|h| Citation::new("memory", h.id.to_string()).labelled(&h.title))
                    .collect(),
            ))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemorySpace {
    Personal,
    Team,
    Company,
}

impl MemorySpace {
    fn label(self) -> &'static str {
        match self {
            Self::Personal => "Mémoire personnelle (visible par vous seul)",
            Self::Team => "Mémoire de votre équipe principale",
            Self::Company => "Mémoire de l’entreprise",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RememberedKind {
    Preference,
    Fact,
    Procedure,
    Decision,
    Lesson,
}

impl From<RememberedKind> for MemoryKind {
    fn from(kind: RememberedKind) -> Self {
        match kind {
            RememberedKind::Preference => Self::Preference,
            RememberedKind::Fact => Self::Fact,
            RememberedKind::Procedure => Self::Procedure,
            RememberedKind::Decision => Self::Decision,
            RememberedKind::Lesson => Self::Lesson,
        }
    }
}

pub struct MemoryRemember {
    pub memory: Arc<dyn MemoryStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MemoryRememberInput {
    space: MemorySpace,
    kind: RememberedKind,
    #[validate(length(min = 3, max = 120))]
    title: String,
    #[validate(length(min = 3, max = 2000))]
    content: String,
}

#[async_trait]
impl TypedTool for MemoryRemember {
    type Input = MemoryRememberInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "memory_remember",
            "Mémorisation",
            "Enregistre une information utile pour les prochaines missions. Espace « personal » : préférences et informations de travail de l’utilisateur (enregistré immédiatement). Espaces « team » et « company » : connaissance partagée, enregistrée comme proposition à valider. N’enregistre jamais de secret, de mot de passe ni de donnée personnelle sensible.",
            ToolRisk::Draft,
        )
        .requires(["agent:use"])
    }

    fn describe_action(&self, input: &MemoryRememberInput) -> Option<String> {
        Some(format!(
            "Mémoriser « {} » ({})",
            input.title,
            input.space.label()
        ))
    }

    async fn run(
        &self,
        input: MemoryRememberInput,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let primary_unit = ctx.subject.org_unit_ids.first().copied();
        let target = match input.space {
            MemorySpace::Personal => MemoryTarget {
                scope: MemoryScope::User,
                org_unit_id: None,
            },
            MemorySpace::Team => MemoryTarget {
                scope: MemoryScope::OrgUnit,
                org_unit_id: primary_unit,
            },
            MemorySpace::Company => MemoryTarget {
                scope: MemoryScope::Tenant,
                org_unit_id: None,
            },
        };
        if target.scope == MemoryScope::OrgUnit && target.org_unit_id.is_none() {
            return Ok(ToolResult::error("Aucune équipe principale"));
        }
        // Proposals to shared spaces are allowed for every member; they only
        // become visible once validated by someone holding memory:write.
        let direct = can_write_memory(ctx.authz.engine(), &ctx.subject, target, &ctx.actor);
        let status = if matches!(input.space, MemorySpace::Personal) && direct {
            MemoryStatus::Active
        } else {
            MemoryStatus::Proposed
        };
        let id = self
            .memory
            .create(NewMemory {
                scope: target.scope,
                owner_user_id: (target.scope == MemoryScope::User).then_some(ctx.subject.user_id),
                org_unit_id: target.org_unit_id,
                kind: input.kind.into(),
                title: input.title.clone(),
                content: input.content,
                sensitivity: Default::default(),
                status,
                source: MemorySource::run(ctx.run_id),
            })
            .await?;
        let active = status == MemoryStatus::Active;
        Ok(ToolResult::ok(json!({
            "id": id,
            "statut": if active { "enregistré" } else { "proposé (validation requise)" },
            "espace": input.space.label(),
        }))
        .with_summary(if active {
            format!("Mémorisé : {}", input.title)
        } else {
            format!("Proposition de mémoire : {}", input.title)
        }))
    }
}
