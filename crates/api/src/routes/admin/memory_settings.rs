//! How the company's memory grows by itself (`settings:manage`,
//! company-wide, ADR-076): whether a lesson is drawn from its runs.

use axum::Json;
use axum::extract::State;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{MemorySettingsUpdate, MemorySettingsView};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::admin::{lesson_capture, set_lesson_capture};

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<MemorySettingsView>> {
    let access = state.access.context(identity).await?;
    let capture_lessons = lesson_capture(&state.db, &access).await?;
    Ok(Json(MemorySettingsView { capture_lessons }))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<MemorySettingsUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    set_lesson_capture(&state.db, &access, body.capture_lessons).await?;
    Ok(Json(Done::OK))
}
