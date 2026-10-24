//! The company's own mission limits (ADR-093), read and set by those who
//! manage its settings, within what the platform allows.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// `GET /admin/mission-settings`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MissionSettingsView {
    /// How many stages the missions entrusted from now on may take.
    pub max_stages: u32,
    /// What they may cost, in micro-dollars.
    pub max_cost_micros: i64,
    /// Whether these are the company's own limits, not the platform's.
    pub own: bool,
    /// The platform's limits, which apply without the company's own.
    pub platform_max_stages: u32,
    pub platform_max_cost_micros: i64,
    /// What a company may set: stages up to this many…
    pub max_stages_ceiling: u32,
    /// …and a cost from this much…
    pub min_cost_micros: i64,
    /// …to this much.
    pub max_cost_ceiling_micros: i64,
}

/// Body of `PUT /admin/mission-settings`: checked against the platform's
/// bounds by the service, which says them in its refusal.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MissionSettingsUpdate {
    pub max_stages: u32,
    pub max_cost_micros: i64,
}
