//! Liveness and readiness: the database answers and whether a model is configured.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::http::ApiResult;

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    sqlx::query("select 1").execute(state.db.pool()).await?;
    Ok(Json(
        json!({ "status": "ok", "llm": state.runs.llm_available() }),
    ))
}
