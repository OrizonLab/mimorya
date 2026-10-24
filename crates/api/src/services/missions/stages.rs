//! A mission's stages (ADR-089). Each is a run of the mission's
//! conversation, queued for a worker as its request is added: the mission
//! itself for the first, a word to go on for the next. As a stage ends, in
//! the transaction that records its outcome, the mission locked: one that
//! spent its budget queues the next while the mission's limits allow, and
//! ends it unfinished beyond; a final answer ends it, done; a failure ends
//! it, failed. A mission stopped meanwhile is left as it is.

use mimorya_agent::runtime::{MISSION_STAGE_BUDGET, RunEvent, RunOutcome};
use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::tools::Citation;
use mimorya_db::TenantTx;
use mimorya_db::notifications::notify_run_queued;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversations::{NewMessage, add_message};
use mimorya_db::repositories::missions::{
    MissionEnd, MissionRow, end_mission, lock_mission, mission_cost, record_stage,
};
use mimorya_db::repositories::run_queue::queue_run;
use mimorya_db::repositories::runs::{NewRun, RunRow, create_run};
use serde_json::json;
use uuid::Uuid;

use super::definition::MissionLimits;
use crate::contract::{MessageMetadata, MissionPart};
use crate::services::event_bus::{BusEvent, RunEventBus};
use crate::services::runs::{RunScope, append_in};

/// The sources carried from stage to stage, the latest kept.
const MAX_CARRIED_SOURCES: usize = 200;

const PAST_ITS_LIMITS: &str = "Les limites de la mission sont atteintes (étapes ou coût) : elle \
     s’arrête inachevée. Son dernier point d’étape dit où elle en est.";

/// The first stage's request: the mission as its owner entrusted it, and
/// how it is carried out.
pub fn brief(mission: &MissionRow, limits: &MissionLimits) -> String {
    let criteria: Vec<String> = mission
        .success_criteria
        .iter()
        .map(|c| format!("- {c}"))
        .collect();
    format!(
        "Mission : {}\n\nObjectif :\n{}\n\nCritères de réussite :\n{}\n\nTu mènes cette mission \
         en arrière-plan, en {} étapes au plus. Chaque étape a un budget limité : une fois \
         atteint, tu rédigeras un point d’étape, dont repartira l’étape suivante. Quand la \
         mission est accomplie, donne le livrable final : il sera vérifié au regard des critères \
         de réussite.",
        mission.title,
        mission.description,
        criteria.join("\n"),
        limits.max_stages,
    )
}

/// What a later stage starts from.
fn resumption(stage: i32, limits: &MissionLimits) -> String {
    format!(
        "Étape {stage} de la mission (sur {} au plus) : reprends à partir de ton dernier point \
         d’étape.",
        limits.max_stages
    )
}

/// Opens stage `stage` of the mission, queued for a worker, its request
/// `text`; the run, and its `run_queued` event to broadcast once committed.
pub async fn open_stage(
    tx: &mut TenantTx,
    bus: &RunEventBus,
    mission: &MissionRow,
    stage: i32,
    text: String,
    part: MissionPart,
) -> anyhow::Result<(RunRow, BusEvent)> {
    let tenant_id = mission.tenant_id;
    let owner = mission.owner_user_id;
    // The mission is its owner's words; a word to go on, the platform's.
    let author = (part == MissionPart::Brief).then_some(owner);
    let metadata = MessageMetadata {
        mission: Some(part),
        ..MessageMetadata::default()
    };
    let request = add_message(
        tx,
        NewMessage {
            tenant_id,
            conversation_id: mission.conversation_id,
            role: "user",
            content: text,
            author_user_id: author,
            run_id: None,
            metadata: serde_json::to_value(metadata)?,
        },
    )
    .await?;
    let autonomy = AutonomyLevel::parse(&mission.autonomy_level)
        .ok_or_else(|| anyhow::anyhow!("unknown autonomy level {}", mission.autonomy_level))?;
    let run = create_run(
        tx,
        NewRun {
            tenant_id,
            conversation_id: Some(mission.conversation_id),
            initiated_by_user_id: owner,
            on_behalf_of_user_id: owner,
            objective: mission.description.clone(),
            success_criteria: mission.success_criteria.clone(),
            autonomy_level: autonomy,
            budget: serde_json::to_value(MISSION_STAGE_BUDGET)?,
            request_message_id: Some(request.id),
            mission_id: Some(mission.id),
        },
    )
    .await?;
    anyhow::ensure!(queue_run(tx, run.id).await?, "stage {} not queued", run.id);
    let queued = append_in(tx, bus, run.id, RunEvent::RunQueued {}).await?;
    notify_run_queued(&mut **tx).await?;
    // Its first stage, its owner sets going; the next ones, the platform.
    let actor = if stage == 1 { "user" } else { "system" };
    let mut started = AuditEntry::new(tenant_id, actor, "agent.run.started", "success")
        .run(run.id)
        .resource("mission", mission.id)
        .metadata(json!({ "stage": stage }));
    started = if stage == 1 {
        started.by_user(owner)
    } else {
        started.on_behalf_of(owner)
    };
    append_audit(tx, started).await?;
    Ok((run, queued))
}

/// What follows for the mission of the run `scope` names as its stage ends
/// with `outcome`, in the transaction recording it: the next stage, queued,
/// and its event to broadcast once committed.
pub async fn after_stage(
    tx: &mut TenantTx,
    bus: &RunEventBus,
    scope: RunScope,
    outcome: &RunOutcome,
) -> anyhow::Result<Option<(Uuid, BusEvent)>> {
    let Some(mission_id) = scope.mission_id else {
        return Ok(None);
    };
    if matches!(outcome, RunOutcome::AwaitingApproval { .. }) {
        return Ok(None);
    }
    let Some(mission) = lock_mission(tx, mission_id).await? else {
        return Ok(None);
    };
    if mission.status != "in_progress" {
        return Ok(None);
    }
    let end = match outcome {
        RunOutcome::AwaitingApproval { .. } => return Ok(None),
        RunOutcome::Completed {
            answer,
            verification,
            ..
        } => MissionEnd {
            status: "done",
            result: Some(answer),
            verification: Some(serde_json::to_value(verification)?),
            error: None,
        },
        RunOutcome::Failed { error, .. } => MissionEnd {
            status: "failed",
            result: None,
            verification: None,
            error: Some(error),
        },
        RunOutcome::Handoff { checkpoint, .. } => {
            let limits: MissionLimits = serde_json::from_value(mission.limits.clone())?;
            let stages = u32::try_from(mission.stages).unwrap_or(u32::MAX);
            let cost = mission_cost(tx, mission.id).await?;
            if stages < limits.max_stages && cost < limits.max_cost_micros {
                let sources = carried(&mission.evidence, &checkpoint.evidence)?;
                record_stage(tx, mission.id, &sources).await?;
                let stage = mission.stages + 1;
                let text = resumption(stage, &limits);
                let (run, queued) =
                    open_stage(tx, bus, &mission, stage, text, MissionPart::Resumption).await?;
                return Ok(Some((run.id, queued)));
            }
            MissionEnd {
                status: "failed",
                result: None,
                verification: None,
                error: Some(PAST_ITS_LIMITS),
            }
        }
    };
    let status = end.status;
    end_mission(tx, mission.id, end).await?;
    let result = if status == "done" {
        "success"
    } else {
        "failure"
    };
    append_audit(
        tx,
        AuditEntry::new(
            mission.tenant_id,
            "system",
            format!("mission.{status}"),
            result,
        )
        .on_behalf_of(mission.owner_user_id)
        .run(scope.run_id)
        .resource("mission", mission.id)
        .metadata(json!({ "stages": mission.stages })),
    )
    .await?;
    Ok(None)
}

/// The sources so far and those of the stage that ended, each once, the
/// latest `MAX_CARRIED_SOURCES` kept.
fn carried(so_far: &serde_json::Value, stage: &[Citation]) -> anyhow::Result<serde_json::Value> {
    let mut sources: Vec<Citation> = serde_json::from_value(so_far.clone())?;
    for citation in stage {
        sources.retain(|known| known.kind != citation.kind || known.id != citation.id);
        sources.push(citation.clone());
    }
    let surplus = sources.len().saturating_sub(MAX_CARRIED_SOURCES);
    sources.drain(..surplus);
    Ok(serde_json::to_value(sources)?)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn carries_each_source_once_the_latest_kept() {
        let so_far = json!([{ "type": "intervention", "id": "INT-1" }]);
        let stage = [
            Citation::new("intervention", "INT-2"),
            Citation::new("intervention", "INT-1"),
        ];
        let sources: Vec<Citation> =
            serde_json::from_value(carried(&so_far, &stage).unwrap()).unwrap();
        let ids: Vec<&str> = sources.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["INT-2", "INT-1"]);

        let many: Vec<Citation> = (0..MAX_CARRIED_SOURCES + 5)
            .map(|i| Citation::new("intervention", format!("INT-{i}")))
            .collect();
        let sources: Vec<Citation> =
            serde_json::from_value(carried(&json!([]), &many).unwrap()).unwrap();
        assert_eq!(sources.len(), MAX_CARRIED_SOURCES);
        assert_eq!(sources[0].id, "INT-5");
    }
}
