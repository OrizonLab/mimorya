//! The company's model escalation (`settings:manage`, company-wide,
//! ADR-207): read with the mechanisms that may be turned on and what they
//! spent this month, set, or removed.

use axum::Json;
use axum::extract::State;
use mimorya_agent::llm::{
    MAX_ESCALATION_BUDGET_MICROS, MAX_ESCALATIONS_PER_RUN, MIN_ESCALATION_BUDGET_MICROS, Mechanism,
};

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{
    EscalationMechanism, MechanismView, ModelEscalationSettings, ModelEscalationUpdate,
    ModelEscalationView,
};
use crate::http::{ApiResult, Body, iso};
use crate::plugins::Auth;
use crate::services::admin::{
    company_model_escalation, reset_company_model_escalation, set_company_model_escalation,
};

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<ModelEscalationView>> {
    let access = state.access.context(identity).await?;
    let overview = company_model_escalation(&state.db, &state.llm, &access).await?;
    Ok(Json(ModelEscalationView {
        main_model: overview.main.map(|main| main.model),
        settings: overview.stored.map(|stored| ModelEscalationSettings {
            model: stored.choice.model,
            after_failure: stored.choice.after_failure,
            router: stored.choice.router,
            advisor: stored.choice.advisor,
            monthly_budget_micros: stored.choice.monthly_budget_micros,
            max_per_run: stored.choice.max_per_run,
            updated_at: iso(&stored.updated_at),
        }),
        problem: overview.problem,
        mechanisms: [
            (EscalationMechanism::AfterFailure, Mechanism::AfterFailure),
            (EscalationMechanism::Router, Mechanism::Router),
            (EscalationMechanism::Advisor, Mechanism::Advisor),
        ]
        .into_iter()
        .map(|(key, mechanism)| MechanismView {
            key,
            label: mechanism.label().to_string(),
            available: mechanism.available(),
            reason: (!mechanism.available()).then(|| "Bientôt disponible".to_string()),
        })
        .collect(),
        spent_micros: overview.spending.spent_micros,
        month_start: iso(&overview.spending.month_start),
        min_budget_micros: MIN_ESCALATION_BUDGET_MICROS,
        max_budget_micros: MAX_ESCALATION_BUDGET_MICROS,
        max_per_run_ceiling: MAX_ESCALATIONS_PER_RUN,
    }))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<ModelEscalationUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    set_company_model_escalation(&state.db, &state.llm, &access, &body).await?;
    Ok(Json(Done::OK))
}

pub async fn reset(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reset_company_model_escalation(&state.db, &state.llm, &access).await?;
    Ok(Json(Done::OK))
}
