//! Tool `team_availability`: busy and free slots of the technicians in scope.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use crate::demo_interventions::dates::{dates_between, is_weekday, iso_date, parse_date};
use crate::demo_interventions::directory::{technician_name, technicians_in_scope};
use crate::demo_interventions::schedule::{WORK_END, WORK_START, busy_slots, to_time};
use crate::demo_interventions::schemas::DATE_RE;
use crate::demo_interventions::store::DemoInterventionsStore;

pub struct TeamAvailability {
    pub store: Arc<DemoInterventionsStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct AvailabilityInput {
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    #[validate(regex(path = *DATE_RE, message = "Format attendu : AAAA-MM-JJ"))]
    from: String,
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    #[validate(regex(path = *DATE_RE, message = "Format attendu : AAAA-MM-JJ"))]
    to: String,
}

#[async_trait]
impl TypedTool for TeamAvailability {
    type Input = AvailabilityInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "team_availability",
            "Disponibilités de l’équipe",
            "Renvoie, pour chaque technicien du périmètre de l’utilisateur, les créneaux occupés et libres (jours ouvrés, 8 h – 17 h) sur une période (31 jours maximum).",
            ToolRisk::Read,
        )
        .requires(["planning:read"])
    }

    async fn run(&self, input: AvailabilityInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let (Some(from), Some(to)) = (parse_date(&input.from), parse_date(&input.to)) else {
            return Ok(ToolResult::error("Date invalide"));
        };
        let technicians = technicians_in_scope(&ctx.authz.scope("planning:read"));
        let days: Vec<_> = dates_between(from, to)
            .into_iter()
            .filter(|d| is_weekday(*d))
            .collect();
        let result: Vec<_> = technicians
            .iter()
            .map(|technician| {
                let jours: Vec<_> = days
                    .iter()
                    .map(|date| {
                        let busy = busy_slots(&self.store, *technician, *date);
                        let mut free = Vec::new();
                        let mut cursor = WORK_START;
                        for slot in &busy {
                            if slot.start > cursor {
                                free.push(format!("{}-{}", to_time(cursor), to_time(slot.start)));
                            }
                            cursor = cursor.max(slot.end);
                        }
                        if cursor < WORK_END {
                            free.push(format!("{}-{}", to_time(cursor), to_time(WORK_END)));
                        }
                        let occupied: Vec<String> = busy
                            .iter()
                            .map(|b| format!("{}-{} {}", to_time(b.start), to_time(b.end), b.label))
                            .collect();
                        json!({ "date": iso_date(*date), "occupe": occupied, "libre": free })
                    })
                    .collect();
                json!({ "technicien": technician_name(*technician), "technicienId": technician, "jours": jours })
            })
            .collect();
        Ok(ToolResult::ok(json!(result)).with_summary(format!(
            "Disponibilités de {} technicien(s)",
            technicians.len()
        )))
    }
}
