//! The company's model escalation (`settings:manage`, company-wide,
//! ADR-207): read with the mechanisms that may be turned on (the advisor
//! where the platform offers it) and what they spent this month, set, or
//! removed.

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
        on_operator_key: overview
            .runs
            .as_ref()
            .is_some_and(|runs| runs.on_operator_key),
        main_model: overview.runs.map(|runs| runs.choice.model),
        settings: overview.stored.map(|stored| ModelEscalationSettings {
            model: stored.choice.model,
            after_failure: stored.choice.after_failure,
            router: stored.choice.router,
            advisor: stored.choice.advisor,
            routes: stored.choice.routes.into(),
            monthly_budget_micros: stored.choice.monthly_budget_micros,
            max_per_run: stored.choice.max_per_run,
            updated_at: iso(&stored.updated_at),
        }),
        problem: overview.problem,
        mechanisms: vec![
            delivered(EscalationMechanism::AfterFailure, Mechanism::AfterFailure),
            delivered(EscalationMechanism::Router, Mechanism::Router),
            // Where the platform offers it; turned on, why it does not serve
            // with the model stored, if so.
            MechanismView {
                key: EscalationMechanism::Advisor,
                label: Mechanism::Advisor.label().to_string(),
                available: overview.advisor.problem().is_none(),
                reason: overview.advisor.problem().or(overview.advisor_problem),
            },
        ],
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

fn delivered(key: EscalationMechanism, mechanism: Mechanism) -> MechanismView {
    MechanismView {
        key,
        label: mechanism.label().to_string(),
        available: mechanism.available(),
        reason: (!mechanism.available()).then(|| "Bientôt disponible".to_string()),
    }
}
