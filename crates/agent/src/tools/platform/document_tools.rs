//! Built-in document tools (ADR-096): search the passages of the documents
//! the user may read, and read one of these documents in order. Both filter
//! with the call's authorization (`document_read_filter`, the rights in force
//! at that step): a document out of reach is answered as absent. What they
//! return is the documents' text, given to the model as data.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;
use validator::Validate;

use crate::runtime::{DocumentPassage, DocumentStore};

const DEFAULT_SEARCH_LIMIT: u32 = 5;
/// The passages a read gives at once.
pub const READ_PASSAGES: u32 = 5;

/// The citation of a document read: its identifier, labelled with its title.
fn cited(document_id: Uuid, title: &str) -> Citation {
    Citation::new("document", document_id.to_string()).labelled(title)
}

/// A passage as the model reads it.
fn shown(passage: &DocumentPassage) -> Value {
    json!({
        "passage": passage.ordinal,
        "page": passage.page,
        "section": passage.heading,
        "texte": passage.content,
    })
}

pub struct DocumentSearch {
    pub documents: Arc<dyn DocumentStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct DocumentSearchInput {
    /// Ce qui est cherché, en langage naturel ou par mots-clés
    #[validate(length(min = 2, max = 300))]
    query: String,
    #[validate(range(min = 1, max = 10))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

#[async_trait]
impl TypedTool for DocumentSearch {
    type Input = DocumentSearchInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "document_search",
            "Recherche dans les documents",
            "Recherche dans les documents déposés que l’utilisateur peut lire (procédures, notices, chartes, contrats…) : renvoie les passages les plus proches de la demande, avec leur document, leur page et leur section. Pour lire la suite d’un passage, utiliser document_read.",
            ToolRisk::Read,
        )
        .requires(["document:read"])
    }

    async fn run(
        &self,
        input: DocumentSearchInput,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let filter = ctx.authz.document_read_filter();
        let limit = input.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        let hits = self.documents.search(&filter, &input.query, limit).await?;
        let content: Vec<Value> = hits
            .iter()
            .map(|hit| {
                let mut passage = shown(&hit.passage);
                passage["document"] = json!(hit.document_id);
                passage["titre"] = json!(hit.title);
                passage
            })
            .collect();
        let mut seen = HashSet::new();
        let citations = hits
            .iter()
            .filter(|hit| seen.insert(hit.document_id))
            .map(|hit| cited(hit.document_id, &hit.title))
            .collect();
        Ok(ToolResult::ok(json!(content))
            .with_summary(format!("{} passage(s) de document trouvé(s)", hits.len()))
            .with_citations(citations))
    }
}

pub struct DocumentRead {
    pub documents: Arc<dyn DocumentStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct DocumentReadInput {
    /// Identifiant du document (champ « document » d’un résultat de recherche)
    document: Uuid,
    /// Premier passage à lire (champ « passage » d’un résultat) ; 0 par défaut
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<u32>,
}

#[async_trait]
impl TypedTool for DocumentRead {
    type Input = DocumentReadInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "document_read",
            "Lecture d’un document",
            "Lit un document déposé que l’utilisateur peut lire, dans l’ordre, cinq passages à la fois à partir du passage donné : pour lire la suite d’un passage trouvé, ou tout un document. Le champ « suite » donne le passage où reprendre.",
            ToolRisk::Read,
        )
        .requires(["document:read"])
    }

    async fn run(&self, input: DocumentReadInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let filter = ctx.authz.document_read_filter();
        let from = input.from.unwrap_or(0);
        // Same answer whether the document does not exist or is out of reach.
        let Some(excerpt) = self
            .documents
            .read(&filter, input.document, from, READ_PASSAGES)
            .await?
        else {
            return Ok(ToolResult::error("Document introuvable ou non accessible"));
        };
        let next = excerpt
            .passages
            .last()
            .map(|passage| passage.ordinal + 1)
            .filter(|next| *next < excerpt.total);
        let summary = match (excerpt.passages.first(), excerpt.passages.last()) {
            (Some(first), Some(last)) => format!(
                "« {} » : passages {} à {} sur {}",
                excerpt.title,
                first.ordinal + 1,
                last.ordinal + 1,
                excerpt.total
            ),
            _ => format!("« {} » : aucun passage à partir de là", excerpt.title),
        };
        Ok(ToolResult::ok(json!({
            "document": input.document,
            "titre": excerpt.title,
            "passages": excerpt.passages.iter().map(shown).collect::<Vec<_>>(),
            "total": excerpt.total,
            "suite": next,
        }))
        .with_summary(summary)
        .with_citations(vec![cited(input.document, &excerpt.title)]))
    }
}
