//! The company's model escalation (ADR-207), read and set by those who
//! manage its settings: its stronger model, the mechanisms that call it,
//! and what they may cost.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// `GET /admin/model-escalation`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelEscalationView {
    /// The model of the company's conversations, if it has its own
    /// provider: without one, nothing may be chosen.
    pub main_model: Option<String>,
    /// The escalation chosen, if any.
    pub settings: Option<ModelEscalationSettings>,
    /// Why the escalation chosen does not serve now (in French): runs keep
    /// the main model.
    pub problem: Option<String>,
    /// Each mechanism, and whether it may be turned on.
    pub mechanisms: Vec<MechanismView>,
    /// What the mechanisms cost since the start of the month, in
    /// micro-dollars.
    pub spent_micros: i64,
    /// The start of the month, in the platform's time zone (RFC 3339).
    pub month_start: String,
    /// A monthly budget goes from this much…
    pub min_budget_micros: i64,
    /// …to this much.
    pub max_budget_micros: i64,
    /// The most escalations a run may be allowed.
    pub max_per_run_ceiling: u32,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelEscalationSettings {
    pub model: String,
    pub after_failure: bool,
    pub router: bool,
    pub advisor: bool,
    pub monthly_budget_micros: i64,
    pub max_per_run: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum EscalationMechanism {
    AfterFailure,
    Router,
    Advisor,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MechanismView {
    pub key: EscalationMechanism,
    pub label: String,
    pub available: bool,
    /// Why it may not be turned on (in French).
    pub reason: Option<String>,
}

/// Body of `PUT /admin/model-escalation`: checked by the service, which
/// says its bounds in its refusal.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelEscalationUpdate {
    pub model: String,
    pub after_failure: bool,
    pub router: bool,
    pub advisor: bool,
    pub monthly_budget_micros: i64,
    pub max_per_run: u32,
}

/// What a company's escalation spent this month, shown with its usage.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EscalationSpending {
    pub budget_micros: i64,
    pub spent_micros: i64,
    pub month_start: String,
}
