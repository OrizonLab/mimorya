//! A person's inbound hooks (ADR-109): what their agent does when another
//! system posts to one, and what became of the last delivery. The token is
//! shown once, when made; the path a system posts to is the same for all.

use mimorya_db::repositories::inbound_hooks::InboundHookRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::stored_text;
use crate::http::iso;

/// `GET /inbound-hooks`: the reader's own, oldest first.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InboundHookList {
    pub hooks: Vec<InboundHook>,
    /// How many one may keep.
    pub max_hooks: u32,
    /// Deliveries a hook takes in an hour.
    pub max_per_hour: u32,
    /// What a delivery may post, at most.
    pub max_body_bytes: u32,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InboundHook {
    pub id: Uuid,
    pub title: String,
    /// Sent as the author's, followed by what was posted.
    pub instruction: String,
    /// Paused when `false`: a delivery answers as to an unknown hook.
    pub enabled: bool,
    /// Where a system posts, under the API (`/api` from outside).
    pub path: String,
    pub last_delivery: Option<LastDelivery>,
    /// Where the last run a delivery started is, while that conversation
    /// exists.
    pub last_conversation_id: Option<Uuid>,
}

/// What became of the last delivery.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LastDelivery {
    pub at: String,
    pub outcome: DeliveryOutcome,
    /// Why it did not run, as the platform says it.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryOutcome {
    /// Its run started, in a new conversation.
    Started,
    /// The hook had taken its deliveries of the hour.
    Limited,
    /// The author's account could not act, or lacks the right, or their
    /// company is suspended.
    Refused,
    /// The run could not start (no model…).
    Failed,
}

impl InboundHook {
    pub fn from_row(row: &InboundHookRow) -> anyhow::Result<Self> {
        let last_delivery = match (&row.last_received_at, &row.last_outcome) {
            (Some(at), Some(outcome)) => Some(LastDelivery {
                at: iso(at),
                outcome: stored_text("inbound hook outcome", outcome)?,
                problem: row.last_problem.clone(),
            }),
            _ => None,
        };
        Ok(Self {
            id: row.id,
            title: row.title.clone(),
            instruction: row.instruction.clone(),
            enabled: row.enabled,
            path: hook_path(row.id),
            last_delivery,
            last_conversation_id: row.last_conversation_id,
        })
    }
}

/// Where a system posts to the hook `id`, under the API.
pub fn hook_path(id: Uuid) -> String {
    format!("/hooks/{id}")
}

/// `POST /inbound-hooks` and `POST /inbound-hooks/{id}/token`: the token,
/// shown this once.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InboundHookToken {
    pub id: Uuid,
    pub path: String,
    /// Sent by the system as `Authorization: Bearer <token>`.
    pub token: String,
}

/// Body of `POST /inbound-hooks`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewInboundHook {
    #[validate(length(max = 400))]
    pub title: String,
    #[validate(length(max = 8000))]
    pub instruction: String,
    /// Paused from the start when `false`; receives by default.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

/// Body of `PATCH /inbound-hooks/{id}`: what changes; the rest stays.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InboundHookUpdate {
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
    pub enabled: Option<bool>,
}
