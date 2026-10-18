//! Approval requests addressed to the signed-in user, and their decisions.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_db::repositories::approvals::{
    APPROVAL_STATUSES, ApprovalRow, list_approvals_for_user,
};
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

use crate::app::AppState;
use crate::http::{ApiResult, Body, PathId, Query, iso, iso_opt, trimmed_opt};
use crate::plugins::Auth;
use crate::services::runs::DecisionInput;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/approvals", get(list))
        .route("/approvals/{id}/decision", post(decide))
}

pub fn approval_view(a: &ApprovalRow) -> Value {
    json!({
        "id": a.id,
        "runId": a.run_id,
        "toolName": a.tool_name,
        "actionSummary": a.action_summary,
        "payload": a.payload,
        "risk": a.risk,
        "reason": a.reason,
        "status": a.status,
        "decisionComment": a.decision_comment,
        "createdAt": iso(&a.created_at),
        "decidedAt": iso_opt(&a.decided_at),
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum StatusFilter {
    #[default]
    Pending,
    All,
}

#[derive(Deserialize, Validate)]
struct ListQuery {
    #[serde(default)]
    status: StatusFilter,
}

async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    let statuses: &[&str] = match query.status {
        StatusFilter::Pending => &["pending"],
        StatusFilter::All => &APPROVAL_STATUSES,
    };
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_approvals_for_user(&mut tx, identity.user_id, statuses).await?;
    tx.commit().await?;
    let approvals: Vec<Value> = rows.iter().map(approval_view).collect();
    Ok(Json(json!({ "approvals": approvals })))
}

#[derive(Deserialize, Validate)]
struct DecisionBody {
    approved: bool,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 500))]
    comment: Option<String>,
}

async fn decide(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<DecisionBody>,
) -> ApiResult<Json<Value>> {
    let result = state
        .runs
        .decide(
            identity,
            DecisionInput {
                approval_id: id,
                approved: body.approved,
                comment: body.comment.filter(|c| !c.is_empty()),
            },
        )
        .await?;
    Ok(Json(json!({
        "approval": approval_view(&result.approval),
        "resumed": result.resumed,
    })))
}
