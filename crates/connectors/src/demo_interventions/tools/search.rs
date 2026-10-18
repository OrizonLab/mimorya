//! Tool `interventions_search`: interventions of a period, filtered by the caller's scope.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use validator::Validate;

use crate::demo_interventions::access::can_read;
use crate::demo_interventions::anomalies::detect_anomalies;
use crate::demo_interventions::dataset::InterventionStatus;
use crate::demo_interventions::dates::{iso_date, parse_date};
use crate::demo_interventions::directory::technician_name;
use crate::demo_interventions::schemas::{DATE_RE, INTERVENTION_ID_PATTERN};
use crate::demo_interventions::store::DemoInterventionsStore;

pub struct InterventionsSearch {
    pub store: Arc<DemoInterventionsStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SearchInput {
    /// Date de début incluse
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    #[validate(regex(path = *DATE_RE, message = "Format attendu : AAAA-MM-JJ"))]
    from: String,
    /// Date de fin incluse
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    #[validate(regex(path = *DATE_RE, message = "Format attendu : AAAA-MM-JJ"))]
    to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<Vec<InterventionStatus>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    technician_id: Option<Uuid>,
}

#[async_trait]
impl TypedTool for InterventionsSearch {
    type Input = SearchInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "interventions_search",
            "Recherche d’interventions",
            "Recherche les interventions (comptes rendus) accessibles à l’utilisateur sur une période. Renvoie un résumé par intervention, avec les anomalies détectées automatiquement. Utiliser interventions_get pour le compte rendu complet.",
            ToolRisk::Read,
        )
        .requires(["intervention:read"])
        .citing(INTERVENTION_ID_PATTERN)
    }

    async fn run(&self, input: SearchInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let (Some(from), Some(to)) = (parse_date(&input.from), parse_date(&input.to)) else {
            return Ok(ToolResult::error("Date invalide"));
        };
        let mut results: Vec<_> = self
            .store
            .interventions
            .iter()
            .filter(|i| i.date >= from && i.date <= to)
            .filter(|i| input.status.as_ref().is_none_or(|s| s.contains(&i.status)))
            .filter(|i| input.technician_id.is_none_or(|t| i.technician_id == t))
            .filter(|i| can_read(ctx, i))
            .collect();
        results.sort_by(|a, b| (a.date, a.start_time).cmp(&(b.date, b.start_time)));
        let interventions: Vec<_> = results
            .iter()
            .map(|i| {
                json!({
                    "id": i.id,
                    "date": iso_date(i.date),
                    "technicien": technician_name(i.technician_id),
                    "technicienId": i.technician_id,
                    "type": i.kind,
                    "client": i.client,
                    "statut": i.status,
                    "piece_en_attente": i.parts_pending,
                    "anomalies": detect_anomalies(i),
                })
            })
            .collect();
        Ok(ToolResult::ok(json!({
            "periode": { "du": input.from, "au": input.to },
            "nombre": results.len(),
            "interventions": interventions,
        }))
        .with_summary(format!("{} intervention(s) trouvée(s)", results.len()))
        .with_citations(
            results
                .iter()
                .map(|i| {
                    Citation::new("intervention", i.id)
                        .labelled(format!("{} — {}", i.kind, i.client))
                })
                .collect(),
        ))
    }
}
