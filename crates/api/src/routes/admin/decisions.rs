//! The company's decision service (`settings:manage`, company-wide,
//! ADR-213): read with what the operator opens and what each usage may be,
//! set, removed, or its engine tested.

use axum::Json;
use axum::extract::State;
use mimorya_agent::decision::EngineKind;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{
    DecisionEngineChoice, DecisionSettingsUpdate, DecisionSettingsView, DecisionTestResult,
    DecisionUsageView,
};
use crate::http::{ApiResult, Body, iso};
use crate::plugins::Auth;
use crate::services::admin::{
    company_decisions, reset_company_decisions, set_company_decisions, test_company_decisions,
};
use crate::services::llm::JEV_CLOSED;

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<DecisionSettingsView>> {
    let access = state.access.context(identity).await?;
    let overview = company_decisions(&state.db, &state.llm, &access).await?;
    let config = state.llm.decision_config();
    Ok(Json(DecisionSettingsView {
        engine: if overview.jev_chosen {
            DecisionEngineChoice::Jev
        } else {
            DecisionEngineChoice::MainModel
        },
        jev_available: config.jev_allowed,
        jev_reason: (!config.jev_allowed).then(|| JEV_CLOSED.to_string()),
        jev_model: config.jev_model.clone(),
        has_jev_key: overview.has_jev_key,
        problem: overview.engines.problem,
        main_model: overview.engines.main_model,
        usages: overview
            .usages
            .into_iter()
            .map(|(usage, policy)| DecisionUsageView {
                key: usage.into(),
                label: usage.label().to_string(),
                available: usage.available(),
                mode: policy.mode.into(),
                threshold: policy.threshold,
            })
            .collect(),
        updated_at: overview.updated_at.as_ref().map(iso),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<DecisionSettingsUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    set_company_decisions(&state.db, &state.llm, &access, &body).await?;
    Ok(Json(Done::OK))
}

pub async fn reset(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reset_company_decisions(&state.db, &state.llm, &access).await?;
    Ok(Json(Done::OK))
}

pub async fn test(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<DecisionTestResult>> {
    let access = state.access.context(identity).await?;
    let tested = test_company_decisions(&state.db, &state.llm, &access).await?;
    Ok(Json(DecisionTestResult {
        engine: match tested.engine {
            EngineKind::Jev => DecisionEngineChoice::Jev,
            EngineKind::MainModel => DecisionEngineChoice::MainModel,
        },
        model: tested.model,
        latency_ms: tested.latency_ms,
        yes: tested.yes,
    }))
}
