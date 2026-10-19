//! Liveness and readiness: the database answers and whether a model is configured.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::app::AppState;
use crate::contract::Health;
use crate::http::ApiResult;

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> ApiResult<Json<Health>> {
    sqlx::query("select 1").execute(state.db.pool()).await?;
    Ok(Json(Health {
        status: "ok".into(),
        llm: state.llm.platform().is_some(),
    }))
}
