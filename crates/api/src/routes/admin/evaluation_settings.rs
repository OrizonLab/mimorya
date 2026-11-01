//! What the company's skill evaluations may cost over thirty days
//! (`settings:manage`, company-wide, ADR-154): read with what was spent,
//! set within the platform's bounds, or back to the platform's.

use axum::Json;
use axum::extract::State;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{EvaluationSettingsUpdate, EvaluationSettingsView};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::admin::{
    company_evaluation_spending, reset_company_evaluation_limit, set_company_evaluation_limit,
};
use crate::services::skill_evaluations::{
    MAX_EVALUATION_LIMIT_MICROS, MIN_EVALUATION_LIMIT_MICROS, PLATFORM_EVALUATION_LIMIT_MICROS,
    SPENDING_WINDOW_DAYS,
};

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<EvaluationSettingsView>> {
    let access = state.access.context(identity).await?;
    let spending = company_evaluation_spending(&state.db, &access).await?;
    Ok(Json(EvaluationSettingsView {
        max_cost_micros: spending.limit_micros,
        own: spending.own,
        spent_micros: spending.spent_micros,
        window_days: SPENDING_WINDOW_DAYS,
        platform_max_cost_micros: PLATFORM_EVALUATION_LIMIT_MICROS,
        min_cost_micros: MIN_EVALUATION_LIMIT_MICROS,
        max_cost_ceiling_micros: MAX_EVALUATION_LIMIT_MICROS,
    }))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<EvaluationSettingsUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    set_company_evaluation_limit(&state.db, &access, body.max_cost_micros).await?;
    Ok(Json(Done::OK))
}

pub async fn reset(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reset_company_evaluation_limit(&state.db, &access).await?;
    Ok(Json(Done::OK))
}
