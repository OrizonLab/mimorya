//! Tool `interventions_get`: full report of one accessible intervention.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Citation, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use crate::demo_interventions::access::can_read;
use crate::demo_interventions::anomalies::detect_anomalies;
use crate::demo_interventions::dates::iso_date;
use crate::demo_interventions::directory::technician_name;
use crate::demo_interventions::schemas::{INTERVENTION_ID_PATTERN, INTERVENTION_ID_RE};
use crate::demo_interventions::store::DemoInterventionsStore;

pub struct InterventionsGet {
    pub store: Arc<DemoInterventionsStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct GetInput {
    #[schemars(regex(pattern = r"^INT-\d{4}$"))]
    #[validate(regex(path = *INTERVENTION_ID_RE))]
    id: String,
}

#[async_trait]
impl TypedTool for InterventionsGet {
    type Input = GetInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "interventions_get",
            "Détail d’une intervention",
            "Renvoie le compte rendu complet d’une intervention accessible à l’utilisateur.",
            ToolRisk::Read,
        )
        .requires(["intervention:read"])
        .citing(INTERVENTION_ID_PATTERN)
    }

    async fn run(&self, input: GetInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        // Same answer whether the record does not exist or is out of scope.
        let Some(i) = self.store.find(&input.id).filter(|i| can_read(ctx, i)) else {
            return Ok(ToolResult::error(format!(
                "Intervention {} introuvable ou non accessible",
                input.id
            )));
        };
        Ok(ToolResult::ok(json!({
            "id": i.id,
            "date": iso_date(i.date),
            "heure": i.start_time,
            "duree_heures": i.duration_hours,
            "technicien": technician_name(i.technician_id),
            "technicienId": i.technician_id,
            "type": i.kind,
            "client": i.client,
            "ville": i.city,
            "statut": i.status,
            "compte_rendu": if i.report.is_empty() { "(vide)" } else { i.report },
            "piece_en_attente": i.parts_pending,
            "signature_client": i.client_signature,
            "anomalies": detect_anomalies(i),
        }))
        .with_summary(format!("{} — {}", i.id, i.kind))
        .with_citations(vec![
            Citation::new("intervention", i.id).labelled(format!("{} — {}", i.kind, i.client)),
        ]))
    }
}
