//! The company's model escalation (ADR-207), read and set by those who
//! manage its settings: its stronger model, the mechanisms that call it,
//! the router's rule for each kind of run, and what they may cost.

use mimorya_agent::llm::{EscalationRoute, EscalationRoutes};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// `GET /admin/model-escalation`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelEscalationView {
    /// The model of the company's runs, if it has one: its own provider's,
    /// or the platform's in the formulas the operator gives it (ADR-209);
    /// without one, nothing may be chosen.
    pub main_model: Option<String>,
    /// Whether the escalation's calls go to the operator's key: they count
    /// in the company's cap or credit as well as in its budget.
    pub on_operator_key: bool,
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
    pub routes: RouterRoutes,
    pub monthly_budget_micros: i64,
    pub max_per_run: u32,
    pub updated_at: String,
}

/// The model a run of a kind starts with while the router is on (step 2):
/// the main one, the escalation one, or the one a classification of its
/// request by the main model names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RouterRoute {
    Main,
    Escalation,
    Classification,
}

/// The router's rule for each kind of run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouterRoutes {
    /// A person's turn of conversation, whatever its channel.
    pub conversation: RouterRoute,
    /// A stage of a mission.
    pub mission: RouterRoute,
    /// A run a scheduled task started.
    pub scheduled_task: RouterRoute,
    /// A run another system started through a hook.
    pub hook: RouterRoute,
}

impl From<RouterRoute> for EscalationRoute {
    fn from(route: RouterRoute) -> Self {
        match route {
            RouterRoute::Main => Self::Main,
            RouterRoute::Escalation => Self::Escalation,
            RouterRoute::Classification => Self::Classification,
        }
    }
}

impl From<EscalationRoute> for RouterRoute {
    fn from(route: EscalationRoute) -> Self {
        match route {
            EscalationRoute::Main => Self::Main,
            EscalationRoute::Escalation => Self::Escalation,
            EscalationRoute::Classification => Self::Classification,
        }
    }
}

impl From<RouterRoutes> for EscalationRoutes {
    fn from(routes: RouterRoutes) -> Self {
        Self {
            conversation: routes.conversation.into(),
            mission: routes.mission.into(),
            scheduled_task: routes.scheduled_task.into(),
            hook: routes.hook.into(),
        }
    }
}

impl From<EscalationRoutes> for RouterRoutes {
    fn from(routes: EscalationRoutes) -> Self {
        Self {
            conversation: routes.conversation.into(),
            mission: routes.mission.into(),
            scheduled_task: routes.scheduled_task.into(),
            hook: routes.hook.into(),
        }
    }
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
    /// Read only while the router is on.
    pub routes: RouterRoutes,
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
