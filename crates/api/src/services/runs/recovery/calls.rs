//! What a stop of the service may have left half done in a run (ADR-053),
//! read back from the database, and how the end of a run that does not go
//! on names it.

use std::collections::HashMap;

use mimorya_agent::runtime::{InterruptedCall, RunCheckpoint, unknown_effect};
use mimorya_core::autonomy::ToolRisk;
use mimorya_db::TenantTx;
use mimorya_db::repositories::approvals::{cancel_pending_approvals, list_approvals_for_run};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connectors::list_connector_instances;
use mimorya_db::repositories::run_events::tool_calls_since;
use mimorya_db::repositories::runs::RunRow;

use crate::services::connectors::read_policies;

/// The current side of the rule a call is judged by
/// (`InterruptedCall::only_read`), for the ends decided here, which start no
/// connector: the risk the administrator set on a tool of the company's
/// enabled connectors, unless they disabled the tool (a run would not have
/// it). What only a started connector tells (the tools it really serves, the
/// risk it declares where no policy sets one) is not known here: such a
/// call is judged by the risk recorded with it. Should two connectors name
/// the same tool, the higher risk counts.
pub(super) async fn risks_set_now(tx: &mut TenantTx) -> anyhow::Result<HashMap<String, ToolRisk>> {
    let mut risks: HashMap<String, ToolRisk> = HashMap::new();
    for instance in list_connector_instances(tx, true).await? {
        // Unreadable policies keep the connector out of runs: its calls are
        // judged by the risk recorded with them.
        let Ok(policies) = read_policies(&instance) else {
            continue;
        };
        let set = policies
            .into_iter()
            .filter(|(_, policy)| policy.enabled != Some(false))
            .filter_map(|(tool, policy)| Some((tool, policy.risk?)));
        for (tool, risk) in set {
            let set = risks.entry(tool).or_insert(risk);
            *set = (*set).max(risk);
        }
    }
    Ok(risks)
}

/// The calls a stop may have left half done, and whether they were approved
/// actions under way (`pending`): the calls started after the saved step,
/// with the risk recorded when they were called, or the actions approved.
pub(super) async fn started_since_saved(
    tx: &mut TenantTx,
    run: &RunRow,
    checkpoint: Option<&RunCheckpoint>,
) -> anyhow::Result<(Vec<InterruptedCall>, bool)> {
    if let Some(pending) = checkpoint.and_then(|c| c.pending.as_ref()) {
        let approvals = list_approvals_for_run(tx, run.id).await?;
        let approved = approvals
            .iter()
            .filter(|a| a.status == "approved")
            .filter(|a| pending.approvals.iter().any(|p| p.approval_id == a.id))
            .map(|a| InterruptedCall {
                tool_name: a.tool_name.clone(),
                title: a.action_summary.clone(),
                risk: None,
            })
            .collect();
        return Ok((approved, true));
    }
    // Without a mark (a state saved before it existed), every call counts.
    let after = run.checkpoint_seq.unwrap_or(0);
    let started = tool_calls_since(tx, run.id, after)
        .await?
        .into_iter()
        .map(|call| InterruptedCall {
            tool_name: call.tool_name,
            title: call.title,
            risk: call
                .risk
                .and_then(|risk| serde_json::from_value(risk.into()).ok()),
        })
        .collect();
    Ok((started, false))
}

/// The lost step's validation requests, cancelled and audited.
pub(super) async fn cancel_lost_validations(tx: &mut TenantTx, run: &RunRow) -> anyhow::Result<()> {
    for approval in cancel_pending_approvals(tx, run.id).await? {
        let entry = AuditEntry::new(run.tenant_id, "system", "approval.cancelled", "success")
            .on_behalf_of(approval.requested_for_user_id)
            .run(run.id)
            .resource("approval", approval.id)
            .reason("Étape perdue lors d’un arrêt du service");
        append_audit(tx, entry).await?;
    }
    Ok(())
}

/// The calls whose effect is not known: approved actions under way, or any
/// call that did not only read, judged as the engine judges it, with the
/// risks set now (`risks_set_now`).
pub(super) fn of_unknown_effect(
    calls: &[InterruptedCall],
    approved: bool,
    set_now: &HashMap<String, ToolRisk>,
) -> Vec<InterruptedCall> {
    calls
        .iter()
        .filter(|call| approved || !call.only_read(set_now.get(&call.tool_name).copied()))
        .cloned()
        .collect()
}

/// `reason`, followed, when actions were under way, by what to check.
pub(super) fn ending(reason: &str, unknown: &[InterruptedCall], approved: bool) -> String {
    if unknown.is_empty() {
        return reason.to_owned();
    }
    format!(
        "{reason} Lors de l’arrêt du service, {}. Vérifiez-le avant de relancer votre demande.",
        unknown_effect(unknown, approved)
    )
}
