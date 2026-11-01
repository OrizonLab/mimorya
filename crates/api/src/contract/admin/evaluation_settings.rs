//! What the company's skill evaluations may cost over thirty days
//! (ADR-154), read and set by those who manage its settings, within what
//! the platform allows.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// `GET /admin/evaluation-settings`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationSettingsView {
    /// What the company's evaluations may cost over the window, in
    /// micro-dollars.
    pub max_cost_micros: i64,
    /// Whether it is the company's own limit, not the platform's.
    pub own: bool,
    /// What they cost over the window, today included.
    pub spent_micros: i64,
    /// The days the limit counts.
    pub window_days: i32,
    /// The platform's limit, which applies without the company's own.
    pub platform_max_cost_micros: i64,
    /// What a company may set: from this much…
    pub min_cost_micros: i64,
    /// …to this much.
    pub max_cost_ceiling_micros: i64,
}

/// Body of `PUT /admin/evaluation-settings`: checked against the platform's
/// bounds by the service, which says them in its refusal.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationSettingsUpdate {
    pub max_cost_micros: i64,
}
