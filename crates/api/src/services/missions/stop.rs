//! Stopping a mission at its owner's request (ADR-089): its stage under way
//! is cancelled, with its requests awaiting a decision, and the mission ends,
//! cancelled. An executor running the stage loses its lease with it, and
//! stops at its next step without writing anything more (ADR-086). The
//! stage is locked before the mission, as when it ends: a stage ending
//! meanwhile is waited for, and the one it queued is the one cancelled. A
//! mission over, or whose last stage failed, cannot be stopped.

use chrono::Utc;
use mimorya_agent::runtime::RunEvent;
use mimorya_core::DomainError;
use mimorya_db::repositories::approvals::cancel_pending_approvals;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversations::{NewMessage, add_message};
use mimorya_db::repositories::missions::{
    MissionEnd, MissionView, active_stage, end_mission, get_mission, lock_mission,
};
use mimorya_db::repositories::runs::{RunPatch, RunStatus, lock_run, update_active_run};
use uuid::Uuid;

use super::entrust::{READ, UNKNOWN_MISSION};
use crate::app::Services;
use crate::contract::{self, MessageMetadata, MissionStatus};
use crate::services::AccessContext;
use crate::services::runs::append_in;

pub const STOPPED: &str = "Mission arrêtée à la demande de son auteur.";
const ALREADY_OVER: &str = "Cette mission est déjà terminée.";
/// Stages that end, and queue the next, while the mission is being stopped.
const ATTEMPTS: usize = 3;

/// The mission, unless it is absent or over: done, failed (its last stage
/// failed included) or cancelled.
fn still_under_way(view: Option<MissionView>) -> anyhow::Result<MissionView> {
    let Some(view) = view else {
        return Err(DomainError::not_found(UNKNOWN_MISSION).into());
    };
    if MissionStatus::of(&view).0.is_over() {
        return Err(DomainError::conflict(ALREADY_OVER).into());
    }
    Ok(view)
}

pub async fn stop_mission(
    services: &Services,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require(READ)?;
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    for _ in 0..ATTEMPTS {
        let mut tx = services.db.begin(tenant_id).await?;
        still_under_way(get_mission(&mut tx, id, user_id).await?)?;
        let seen = active_stage(&mut tx, id).await?;
        if let Some(stage) = &seen {
            lock_run(&mut tx, stage.id).await?;
        }
        lock_mission(&mut tx, id).await?;
        // As it stands now, its stage and itself locked.
        let view = still_under_way(get_mission(&mut tx, id, user_id).await?)?;
        let stage = active_stage(&mut tx, id).await?;
        if stage.as_ref().map(|s| s.id) != seen.as_ref().map(|s| s.id) {
            // A stage ended meanwhile, and queued the next: again.
            continue;
        }
        let mut finished = None;
        if let Some(stage) = stage {
            let cancelled = RunPatch {
                status: Some(RunStatus::Cancelled),
                error: Some(STOPPED.into()),
                finished_at: Some(Utc::now()),
                ..RunPatch::default()
            };
            if update_active_run(&mut tx, stage.id, cancelled).await? {
                cancel_pending_approvals(&mut tx, stage.id).await?;
                let metadata = MessageMetadata {
                    status: Some(contract::RunStatus::Cancelled),
                    ..MessageMetadata::default()
                };
                add_message(
                    &mut tx,
                    NewMessage {
                        tenant_id,
                        conversation_id: view.mission.conversation_id,
                        role: "assistant",
                        content: STOPPED.into(),
                        author_user_id: None,
                        run_id: Some(stage.id),
                        metadata: serde_json::to_value(metadata)?,
                    },
                )
                .await?;
                let end = RunEvent::RunFinished {
                    status: RunStatus::Cancelled.as_str().into(),
                    error: Some(STOPPED.into()),
                };
                let event = append_in(&mut tx, &services.bus, stage.id, end).await?;
                finished = Some((stage.id, event));
            }
        }
        let end = MissionEnd {
            status: "cancelled",
            result: None,
            verification: None,
            error: Some(STOPPED),
        };
        end_mission(&mut tx, id, end).await?;
        let mut stopped = AuditEntry::new(tenant_id, "user", "mission.cancelled", "success")
            .by_user(user_id)
            .resource("mission", id);
        if let Some((run_id, _)) = &finished {
            stopped = stopped.run(*run_id);
        }
        append_audit(&mut tx, stopped).await?;
        tx.commit().await?;
        if let Some((run_id, event)) = finished {
            services.bus.publish(run_id, event);
        }
        return Ok(());
    }
    anyhow::bail!("mission {id} kept going on while being stopped")
}
