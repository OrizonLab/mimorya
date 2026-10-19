//! Approval requests addressed to the signed-in user, and their decisions.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_db::repositories::approvals::{APPROVAL_STATUSES, list_approvals_for_user};
use serde::Deserialize;
use validator::Validate;

use crate::app::AppState;
use crate::contract::{Approval, ApprovalDecided, ApprovalList};
use crate::http::{ApiResult, Body, PathId, Query, trimmed_opt};
use crate::plugins::Auth;
use crate::services::runs::DecisionInput;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/approvals", get(list))
        .route("/approvals/{id}/decision", post(decide))
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
) -> ApiResult<Json<ApprovalList>> {
    let statuses: &[&str] = match query.status {
        StatusFilter::Pending => &["pending"],
        StatusFilter::All => &APPROVAL_STATUSES,
    };
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_approvals_for_user(&mut tx, identity.user_id, statuses).await?;
    tx.commit().await?;
    Ok(Json(ApprovalList {
        approvals: rows
            .iter()
            .map(Approval::from_row)
            .collect::<anyhow::Result<_>>()?,
    }))
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
) -> ApiResult<Json<ApprovalDecided>> {
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
    Ok(Json(ApprovalDecided {
        approval: Approval::from_row(&result.approval)?,
        resumed: result.resumed,
    }))
}
