//! Runs executed on behalf of the signed-in user, with their trace.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_db::repositories::approvals::list_approvals_for_run;
use mimorya_db::repositories::run_events::list_run_events;
use mimorya_db::repositories::runs::{get_run, list_runs};
use mimorya_db::repositories::tool_invocations::list_tool_invocations;
use serde_json::{Value, json};

use super::approvals::approval_view;
use crate::app::AppState;
use crate::http::{ApiResult, PathId, iso, iso_opt};
use crate::plugins::Auth;

const RUN_PAGE: i64 = 50;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/runs", get(list))
        .route("/runs/{id}", get(detail))
}

async fn list(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_runs(&mut tx, identity.user_id, RUN_PAGE).await?;
    tx.commit().await?;
    let runs: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "conversationId": r.conversation_id,
                "objective": r.objective,
                "status": r.status,
                "usage": r.usage,
                "error": r.error,
                "createdAt": iso(&r.created_at),
                "finishedAt": iso_opt(&r.finished_at),
            })
        })
        .collect();
    Ok(Json(json!({ "runs": runs })))
}

async fn detail(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Value>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let run = get_run(&mut tx, id)
        .await?
        .filter(|r| r.on_behalf_of_user_id == identity.user_id)
        .ok_or_else(|| DomainError::not_found("Exécution introuvable"))?;
    let tools = list_tool_invocations(&mut tx, id).await?;
    let approvals = list_approvals_for_run(&mut tx, id).await?;
    let events = list_run_events(&mut tx, id, 0).await?;
    tx.commit().await?;

    let tools: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "toolName": t.tool_name,
                "decision": t.decision,
                "decisionReason": t.decision_reason,
                "status": t.status,
                "outputSummary": t.output_summary,
                "durationMs": t.duration_ms,
                "createdAt": iso(&t.created_at),
            })
        })
        .collect();
    let approvals: Vec<Value> = approvals.iter().map(approval_view).collect();
    let events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "seq": e.seq,
                "type": e.event_type,
                "data": e.data,
                "createdAt": iso(&e.created_at),
            })
        })
        .collect();
    Ok(Json(json!({
        "run": {
            "id": run.id,
            "conversationId": run.conversation_id,
            "objective": run.objective,
            "status": run.status,
            "autonomyLevel": run.autonomy_level,
            "usage": run.usage,
            "budget": run.budget,
            "verification": run.verification,
            "error": run.error,
            "createdAt": iso(&run.created_at),
            "finishedAt": iso_opt(&run.finished_at),
        },
        "tools": tools,
        "approvals": approvals,
        "events": events,
    })))
}
