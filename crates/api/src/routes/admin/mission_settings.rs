//! The company's own mission limits (`settings:manage`, company-wide,
//! ADR-093): read, set within the platform's bounds, or back to the
//! platform's.

use axum::Json;
use axum::extract::State;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{MissionSettingsUpdate, MissionSettingsView};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::admin::{
    company_mission_limits, reset_company_mission_limits, set_company_mission_limits,
};
use crate::services::missions::{
    MAX_COST_CEILING_MICROS, MAX_STAGES_CEILING, MIN_COST_MICROS, MISSION_LIMITS,
};

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<MissionSettingsView>> {
    let access = state.access.context(identity).await?;
    let (limits, own) = company_mission_limits(&state.db, &access).await?;
    Ok(Json(MissionSettingsView {
        max_stages: limits.max_stages,
        max_cost_micros: limits.max_cost_micros,
        own,
        platform_max_stages: MISSION_LIMITS.max_stages,
        platform_max_cost_micros: MISSION_LIMITS.max_cost_micros,
        max_stages_ceiling: MAX_STAGES_CEILING,
        min_cost_micros: MIN_COST_MICROS,
        max_cost_ceiling_micros: MAX_COST_CEILING_MICROS,
    }))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<MissionSettingsUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    set_company_mission_limits(&state.db, &access, body.max_stages, body.max_cost_micros).await?;
    Ok(Json(Done::OK))
}

pub async fn reset(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reset_company_mission_limits(&state.db, &access).await?;
    Ok(Json(Done::OK))
}
