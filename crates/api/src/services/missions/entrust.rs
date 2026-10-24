//! A person's own missions (ADR-089): entrusted, listed and read by their
//! owner only; anyone else's answers as absent. Entrusting one needs, as a
//! conversation turn does, a model, an active company and the right to use
//! the agent, and the right to entrust missions (`mission:create`); at most
//! `MAX_UNDER_WAY` under way each. It opens the mission's conversation,
//! named after it, and queues its first stage. Following one's missions
//! needs `mission:read`. Each is audited, never its text.

use mimorya_core::DomainError;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversations::create_conversation;
use mimorya_db::repositories::missions::{
    MissionView, NewMission, count_missions_under_way, create_mission, get_mission, list_missions,
};
use uuid::Uuid;

use super::definition::CheckedMission;
use super::limits::company_limits;
use super::stages::{brief, open_stage};
use crate::app::Services;
use crate::contract::MissionPart;
use crate::services::runs::tenant_autonomy_level;
use crate::services::{AccessContext, AuthIdentity};

/// The missions one person may have under way at once.
pub const MAX_UNDER_WAY: i64 = 3;
/// The missions listed, newest first.
const LISTED: i64 = 100;
const CREATE: &str = "mission:create";
pub const READ: &str = "mission:read";

pub const UNKNOWN_MISSION: &str = "Mission introuvable";

/// A mission as its owner writes it.
pub struct MissionInput {
    pub title: String,
    pub objective: String,
    pub criteria: Vec<String>,
}

/// Entrusts the mission to the agent of `identity`; its identifier.
pub async fn entrust_mission(
    services: &Services,
    identity: AuthIdentity,
    input: MissionInput,
) -> anyhow::Result<Uuid> {
    let access = services.runs.ready_to_run(identity).await?;
    access.require(CREATE)?;
    let checked = CheckedMission::check(&input.title, &input.objective, &input.criteria)?;
    let autonomy = tenant_autonomy_level(&access);
    let (tenant_id, user_id) = (identity.tenant_id, identity.user_id);
    let mut tx = services.db.begin(tenant_id).await?;
    if count_missions_under_way(&mut tx, user_id).await? >= MAX_UNDER_WAY {
        let too_many = format!(
            "Vous avez déjà {MAX_UNDER_WAY} missions en cours : attendez la fin de l’une \
             d’elles, ou arrêtez-la."
        );
        return Err(DomainError::conflict(too_many).into());
    }
    // The company's limits as they stand, kept on the mission (ADR-093).
    let limits = company_limits(&mut tx).await?;
    let conversation =
        create_conversation(&mut tx, tenant_id, user_id, Some(&checked.title)).await?;
    let mission = create_mission(
        &mut tx,
        NewMission {
            tenant_id,
            owner_user_id: user_id,
            conversation_id: conversation.id,
            title: &checked.title,
            description: &checked.objective,
            success_criteria: &checked.criteria,
            autonomy_level: autonomy.as_str(),
            limits: serde_json::to_value(limits)?,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        AuditEntry::new(tenant_id, "user", "mission.entrusted", "success")
            .by_user(user_id)
            .resource("mission", mission.id),
    )
    .await?;
    let text = brief(&mission, &limits);
    let bus = &services.bus;
    let brief_part = MissionPart::Brief;
    let (run, queued) = open_stage(&mut tx, bus, &mission, 1, text, brief_part, 0).await?;
    tx.commit().await?;
    bus.publish(run.id, queued);
    Ok(mission.id)
}

/// The missions of the reader, newest first.
pub async fn missions(
    services: &Services,
    access: &AccessContext,
) -> anyhow::Result<Vec<MissionView>> {
    access.require(READ)?;
    let mut tx = services.db.begin(access.subject.tenant_id).await?;
    let missions = list_missions(&mut tx, access.subject.user_id, LISTED).await?;
    tx.commit().await?;
    Ok(missions)
}

/// The reader's mission `id`; anyone else's answers as absent.
pub async fn mission(
    services: &Services,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<MissionView> {
    access.require(READ)?;
    let mut tx = services.db.begin(access.subject.tenant_id).await?;
    let mission = get_mission(&mut tx, id, access.subject.user_id).await?;
    tx.commit().await?;
    mission.ok_or_else(|| DomainError::not_found(UNKNOWN_MISSION).into())
}
