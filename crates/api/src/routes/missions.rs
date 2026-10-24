//! The signed-in person's missions (ADR-089): work entrusted to their agent,
//! carried out in the background in stages, listed, read and stopped. Their
//! own resource: anyone else's answers as absent.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::app::AppState;
use crate::contract::{Created, Done, Mission, MissionList, NewMission};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::missions::{
    MAX_UNDER_WAY, MissionInput, entrust_mission, mission, missions, stop_mission,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/missions", get(list).post(entrust))
        .route("/missions/{id}", get(read))
        .route("/missions/{id}/stop", post(stop))
}

async fn list(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<MissionList>> {
    let access = state.access.context(identity).await?;
    let views = missions(&state, &access).await?;
    let missions = views
        .iter()
        .map(Mission::from_view)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Json(MissionList {
        missions,
        max_under_way: u32::try_from(MAX_UNDER_WAY).unwrap_or(u32::MAX),
    }))
}

async fn entrust(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewMission>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let input = MissionInput {
        title: body.title,
        objective: body.objective,
        criteria: body.success_criteria,
    };
    let id = entrust_mission(&state, identity, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

async fn read(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Mission>> {
    let access = state.access.context(identity).await?;
    let view = mission(&state, &access, id).await?;
    Ok(Json(Mission::from_view(&view)?))
}

async fn stop(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    stop_mission(&state, &access, id).await?;
    Ok(Json(Done::OK))
}
