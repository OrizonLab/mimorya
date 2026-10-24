//! A person's missions (ADR-089): what they entrusted their agent, how far
//! it went, what it cost, and how it ended.

use mimorya_agent::runtime::VerificationReport;
use mimorya_db::repositories::missions::MissionView;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::Verification;
use crate::http::iso;
use crate::services::missions::MissionLimits;

/// `GET /missions`: the reader's own, newest first.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MissionList {
    pub missions: Vec<Mission>,
    /// How many may be under way at once, per person.
    pub max_under_way: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    /// A stage is queued or running.
    InProgress,
    /// A stage waits for a decision on one of its actions.
    AwaitingApproval,
    /// Its final answer is given.
    Done,
    /// It stopped unfinished: a stage failed, or its limits were reached.
    Failed,
    /// Its owner stopped it.
    Cancelled,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Mission {
    pub id: Uuid,
    pub title: String,
    pub objective: String,
    pub success_criteria: Vec<String>,
    pub status: MissionStatus,
    /// Where its stages run, and its answer is given.
    pub conversation_id: Uuid,
    /// Stages started, and how many it may start.
    pub stages: u32,
    pub max_stages: u32,
    /// What its stages cost so far, and how much they may.
    pub cost_micros: i64,
    pub max_cost_micros: i64,
    /// Its final answer, once done.
    pub result: Option<String>,
    /// How its final answer was reviewed against its criteria.
    pub verification: Option<Verification>,
    /// Why it stopped unfinished.
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

impl Mission {
    pub fn from_view(view: &MissionView) -> anyhow::Result<Self> {
        let mission = &view.mission;
        let limits: MissionLimits = serde_json::from_value(mission.limits.clone())?;
        let (status, error) = match mission.status.as_str() {
            "done" => (MissionStatus::Done, None),
            "cancelled" => (MissionStatus::Cancelled, mission.error.clone()),
            "failed" => (MissionStatus::Failed, mission.error.clone()),
            // Under way: as its last stage stands.
            _ => match view.stage_status.as_deref() {
                Some("awaiting_approval") => (MissionStatus::AwaitingApproval, None),
                Some("failed" | "cancelled") => (MissionStatus::Failed, view.stage_error.clone()),
                _ => (MissionStatus::InProgress, None),
            },
        };
        let verification = match &mission.verification {
            Some(value) => {
                let report: VerificationReport = serde_json::from_value(value.clone())?;
                Some(Verification::from(&report))
            }
            None => None,
        };
        Ok(Self {
            id: mission.id,
            title: mission.title.clone(),
            objective: mission.description.clone(),
            success_criteria: mission.success_criteria.clone(),
            status,
            conversation_id: mission.conversation_id,
            stages: u32::try_from(mission.stages).unwrap_or_default(),
            max_stages: limits.max_stages,
            cost_micros: view.cost_micros,
            max_cost_micros: limits.max_cost_micros,
            result: mission.result.clone(),
            verification,
            error,
            created_at: iso(&mission.created_at),
            finished_at: mission.finished_at.as_ref().map(iso),
        })
    }
}

/// Body of `POST /missions`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewMission {
    #[validate(length(max = 400))]
    pub title: String,
    #[validate(length(max = 8000))]
    pub objective: String,
    #[validate(length(max = 16))]
    pub success_criteria: Vec<String>,
}
