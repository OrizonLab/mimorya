//! A person's scheduled tasks (ADR-080): what they ask their agent, when,
//! and what became of the last occurrence.

use mimorya_db::repositories::scheduled_tasks::ScheduledTaskRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::stored_text;
use crate::http::iso;

/// `GET /scheduled-tasks`: the reader's own, oldest first.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskList {
    pub tasks: Vec<ScheduledTask>,
    /// How many one may keep.
    pub max_tasks: u32,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTask {
    pub id: Uuid,
    pub title: String,
    /// Sent as the author's, each time.
    pub instruction: String,
    /// ISO days of the week: 1 is Monday.
    pub weekdays: Vec<u8>,
    /// `HH:MM`, in `timeZone`.
    pub time: String,
    pub time_zone: String,
    /// Paused when `false`.
    pub enabled: bool,
    pub next_run_at: Option<String>,
    pub last_occurrence: Option<LastOccurrence>,
    /// Where the last run an occurrence started is, while that conversation
    /// exists: an occurrence does not start while that run is under way.
    pub last_conversation_id: Option<Uuid>,
}

/// What became of the last occurrence.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LastOccurrence {
    pub at: String,
    pub outcome: OccurrenceOutcome,
    /// Why it did not run, as the platform says it.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceOutcome {
    /// Its run started, in a new conversation.
    Started,
    /// The last occurrence's run was not over.
    StillRunning,
    /// Not started within the hour after its time (the service was
    /// stopped).
    Missed,
    /// The author's account could not act, or lacks the right, or their
    /// company is suspended.
    Refused,
    /// The run could not start (no model…).
    Failed,
}

impl ScheduledTask {
    pub fn from_row(row: &ScheduledTaskRow) -> anyhow::Result<Self> {
        let last_occurrence = match (&row.last_run_at, &row.last_outcome) {
            (Some(at), Some(outcome)) => Some(LastOccurrence {
                at: iso(at),
                outcome: stored_text("scheduled task outcome", outcome)?,
                problem: row.last_problem.clone(),
            }),
            _ => None,
        };
        Ok(Self {
            id: row.id,
            title: row.title.clone(),
            instruction: row.instruction.clone(),
            weekdays: row
                .weekdays
                .iter()
                .filter_map(|d| u8::try_from(*d).ok())
                .collect(),
            time: row.time_of_day.clone(),
            time_zone: row.time_zone.clone(),
            enabled: row.next_run_at.is_some(),
            next_run_at: row.next_run_at.as_ref().map(iso),
            last_occurrence,
            last_conversation_id: row.last_conversation_id,
        })
    }
}

/// Body of `POST /scheduled-tasks`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewScheduledTask {
    #[validate(length(max = 400))]
    pub title: String,
    #[validate(length(max = 8000))]
    pub instruction: String,
    #[validate(length(max = 7))]
    pub weekdays: Vec<u8>,
    #[validate(length(max = 5))]
    pub time: String,
    #[validate(length(max = 64))]
    pub time_zone: String,
    /// Paused from the start when `false`; runs by default.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

/// Body of `PATCH /scheduled-tasks/{id}`: what changes; the rest stays.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScheduledTaskUpdate {
    #[serde(default)]
    #[ts(optional)]
    #[validate(length(max = 400))]
    pub title: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    #[validate(length(max = 8000))]
    pub instruction: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    #[validate(length(max = 7))]
    pub weekdays: Option<Vec<u8>>,
    #[serde(default)]
    #[ts(optional)]
    #[validate(length(max = 5))]
    pub time: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    #[validate(length(max = 64))]
    pub time_zone: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}
