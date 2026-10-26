//! Tool `planning_propose`: validated draft of a follow-up planning (no calendar change).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{
    Citation, DeliverableInput, ToolContext, ToolInfo, ToolResult, TypedTool,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use validator::Validate;

use crate::demo_interventions::access::can_read;
use crate::demo_interventions::dates::{is_weekday, parse_date};
use crate::demo_interventions::directory::{technician_name, technicians_in_scope};
use crate::demo_interventions::schedule::{
    Busy, WORK_END, WORK_START, busy_slots, end_of, to_minutes,
};
use crate::demo_interventions::schemas::{INTERVENTION_ID_PATTERN, ProposedSlot};
use crate::demo_interventions::store::DemoInterventionsStore;

pub struct PlanningPropose {
    pub store: Arc<DemoInterventionsStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct ProposeInput {
    #[validate(length(min = 3, max = 120))]
    title: String,
    #[validate(length(min = 1, max = 30), nested)]
    items: Vec<ProposedSlot>,
}

impl PlanningPropose {
    /// Problems that make the proposal invalid, in French.
    fn check(&self, items: &[ProposedSlot], ctx: &ToolContext) -> Vec<String> {
        let allowed: HashSet<_> = technicians_in_scope(&ctx.authz.scope("planning:propose"))
            .into_iter()
            .collect();
        let mut problems = Vec::new();
        let mut planned: Vec<(uuid::Uuid, chrono::NaiveDate, Busy)> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let label = format!("Créneau {} ({})", index + 1, item.intervention_id);
            let accessible = self
                .store
                .find(&item.intervention_id)
                .is_some_and(|i| can_read(ctx, i));
            if !accessible {
                problems.push(format!(
                    "{label} : intervention introuvable ou non accessible"
                ));
            }
            if !allowed.contains(&item.technician_id) {
                problems.push(format!(
                    "{label} : technicien hors de votre périmètre de planification"
                ));
            }
            let Some(date) = parse_date(&item.date) else {
                problems.push(format!("{label} : date {} invalide", item.date));
                continue;
            };
            if !is_weekday(date) {
                problems.push(format!("{label} : {} n’est pas un jour ouvré", item.date));
            }
            let start = to_minutes(&item.start_time);
            let end = end_of(start, item.duration_hours);
            if start < WORK_START || end > WORK_END {
                problems.push(format!("{label} : en dehors des horaires 08:00-17:00"));
            }
            for conflict in busy_slots(&self.store, item.technician_id, date, &|i| can_read(ctx, i))
                .iter()
                .filter(|b| b.overlaps(start, end))
            {
                problems.push(format!("{label} : chevauche « {} »", conflict.label));
            }
            let clashes = planned
                .iter()
                .filter(|(t, d, b)| {
                    *t == item.technician_id && *d == date && b.overlaps(start, end)
                })
                .count();
            for _ in 0..clashes {
                problems.push(format!(
                    "{label} : chevauche un autre créneau de la proposition"
                ));
            }
            planned.push((
                item.technician_id,
                date,
                Busy {
                    start,
                    end,
                    label: String::new(),
                },
            ));
        }
        problems
    }
}

#[async_trait]
impl TypedTool for PlanningPropose {
    type Input = ProposeInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "planning_propose",
            "Proposition de planning",
            "Crée une proposition de planning de suivi (brouillon, sans effet sur les agendas). Contrôle : intervention accessible, technicien du périmètre, jour ouvré, horaires 8 h – 17 h, absence de chevauchement. Renvoie l’identifiant de la proposition ou la liste des problèmes à corriger.",
            ToolRisk::Draft,
        )
        .requires(["planning:propose", "intervention:read"])
        .citing(INTERVENTION_ID_PATTERN)
    }

    async fn run(&self, input: ProposeInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let problems = self.check(&input.items, ctx);
        if !problems.is_empty() {
            let count = problems.len();
            return Ok(ToolResult {
                content: json!({ "erreur": "Proposition non valide", "problemes": problems }),
                is_error: true,
                summary: Some(format!("{count} problème(s) à corriger")),
                ..ToolResult::default()
            });
        }

        let items: Vec<Value> = input
            .items
            .iter()
            .map(|item| {
                let mut value = serde_json::to_value(item).expect("serialisable slot");
                value["technicien"] = json!(technician_name(item.technician_id));
                value
            })
            .collect();
        let deliverable = ctx
            .deliverables
            .create(DeliverableInput {
                kind: "planning_proposal".into(),
                title: input.title.clone(),
                content: json!({ "items": items }),
                org_unit_id: ctx.subject.org_unit_ids.first().copied(),
            })
            .await?;
        let mut citations =
            vec![Citation::new("deliverable", deliverable.id.to_string()).labelled(&input.title)];
        citations.extend(
            input
                .items
                .iter()
                .map(|i| Citation::new("intervention", &i.intervention_id)),
        );
        Ok(ToolResult::ok(json!({
            "propositionId": deliverable.id,
            "statut": "brouillon",
            "creneaux": input.items.len(),
            "note": "Aucun agenda n’est modifié tant que la proposition n’est pas appliquée (planning_apply, soumis à validation).",
        }))
        .with_summary(format!("Proposition « {} » ({} créneau(x))", input.title, input.items.len()))
        .with_citations(citations))
    }
}
