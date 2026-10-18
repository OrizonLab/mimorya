//! Tool `planning_apply`: applies a draft planning to the calendars (requires validation).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{DeliverableStatus, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use validator::Validate;

use crate::demo_interventions::access::can_read;
use crate::demo_interventions::dates::parse_date;
use crate::demo_interventions::directory::technicians_in_scope;
use crate::demo_interventions::store::{DemoInterventionsStore, ScheduledSlot};

pub struct PlanningApply {
    pub store: Arc<DemoInterventionsStore>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ApplyInput {
    #[validate(length(min = 1))]
    proposal_id: String,
}

/// A slot as stored in the proposal (with the technician's display name).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSlot {
    intervention_id: String,
    technician_id: Uuid,
    date: String,
    start_time: String,
    duration_hours: f64,
}

#[derive(Deserialize)]
struct ProposalContent {
    items: Vec<StoredSlot>,
}

impl PlanningApply {
    /// Interventions of the slots the subject may not apply.
    fn out_of_scope(&self, items: &[StoredSlot], ctx: &ToolContext) -> Vec<String> {
        let technicians: HashSet<Uuid> = technicians_in_scope(&ctx.authz.scope("planning:apply"))
            .into_iter()
            .collect();
        items
            .iter()
            .filter(|item| {
                let readable = self
                    .store
                    .find(&item.intervention_id)
                    .is_some_and(|i| can_read(ctx, i));
                !readable || !technicians.contains(&item.technician_id)
            })
            .map(|item| item.intervention_id.clone())
            .collect()
    }
}

#[async_trait]
impl TypedTool for PlanningApply {
    type Input = ApplyInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "planning_apply",
            "Application d’un planning",
            "Applique une proposition de planning aux agendas des techniciens. Action soumise à la validation de l’utilisateur.",
            ToolRisk::Write,
        )
        .requires(["planning:apply"])
    }

    fn describe_action(&self, input: &ApplyInput) -> Option<String> {
        Some(format!(
            "Appliquer la proposition de planning {} aux agendas de l’équipe",
            input.proposal_id
        ))
    }

    async fn run(&self, input: ApplyInput, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let not_found = || Ok(ToolResult::error("Proposition introuvable"));
        let Ok(id) = Uuid::parse_str(&input.proposal_id) else {
            return not_found();
        };
        let Some(proposal) = ctx.deliverables.get(id).await? else {
            return not_found();
        };
        if proposal.kind != "planning_proposal" || proposal.owner_user_id != ctx.subject.user_id {
            return not_found();
        }
        if !matches!(
            proposal.status,
            DeliverableStatus::Draft | DeliverableStatus::Approved
        ) {
            return Ok(ToolResult::error(format!(
                "Proposition déjà {}",
                proposal.status.as_str()
            )));
        }
        let content: ProposalContent = serde_json::from_value(proposal.content)?;
        // The proposal was checked against the propose scope; applying it must
        // fit the (possibly narrower) apply scope, slot by slot.
        let out_of_scope = self.out_of_scope(&content.items, ctx);
        if !out_of_scope.is_empty() {
            return Ok(ToolResult::error(format!(
                "Proposition hors de votre périmètre d’application ({}) : rien n’a été appliqué",
                out_of_scope.join(", ")
            )));
        }
        let count = content.items.len();
        let slots = content
            .items
            .into_iter()
            .filter_map(|item| {
                Some(ScheduledSlot {
                    technician_id: item.technician_id,
                    date: parse_date(&item.date)?,
                    start_time: item.start_time,
                    duration_hours: item.duration_hours,
                    label: format!("Suivi {}", item.intervention_id),
                })
            })
            .collect();
        self.store.apply(slots);
        ctx.deliverables
            .set_status(proposal.id, DeliverableStatus::Applied)
            .await?;
        Ok(ToolResult::ok(
            json!({ "propositionId": proposal.id, "statut": "appliquée", "creneaux": count }),
        )
        .with_summary(format!("Planning appliqué ({count} créneau(x))")))
    }
}
