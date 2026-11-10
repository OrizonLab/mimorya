//! How long the company keeps its idle conversations, its episodes and the
//! personal memories of its accounts gone (`retention:manage`,
//! company-wide, ADR-195): read with what the purge applies now and the
//! platform's bounds, set within them.

use axum::Json;
use axum::extract::State;
use mimorya_core::retention::SHORTENING_DELAY_DAYS;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{RetentionDurationView, RetentionUpdate, RetentionView};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::admin::{RetentionDuration, company_retention, set_company_retention};

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<RetentionView>> {
    let access = state.access.context(identity).await?;
    let durations = company_retention(&state.db, &access).await?;
    Ok(Json(RetentionView {
        durations: durations.into_iter().map(view).collect(),
        shortening_delay_days: SHORTENING_DELAY_DAYS,
    }))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<RetentionUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    set_company_retention(&state.db, &access, body.durations()).await?;
    Ok(Json(Done::OK))
}

fn view(duration: RetentionDuration) -> RetentionDurationView {
    let kind = duration.kind;
    RetentionDurationView {
        kind: kind.into(),
        label: kind.label().to_string(),
        unit: kind.unit().into(),
        duration: duration.duration,
        in_effect: duration.in_effect,
        own: duration.own,
        shortening_applies_at: duration.shortening_applies_at,
        platform: kind.platform(),
        min: kind.min(),
        max: kind.max(),
    }
}
