//! Approval requests made for the signed-in user or that they decide as a
//! manager (ADR-059), and their decisions.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_db::repositories::approval_views::{get_approval_view, list_approval_views_for_user};
use mimorya_db::repositories::approvals::APPROVAL_STATUSES;
use serde::Deserialize;
use validator::Validate;

use crate::app::AppState;
use crate::contract::{Approval, ApprovalDecided, ApprovalList, ApprovalVerdict};
use crate::http::{ApiResult, Body, PathId, Query};
use crate::plugins::Auth;
use crate::services::runs::{DecisionInput, can_decide, shown_to};

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
    let viewer = state.access.context(identity).await?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let views = list_approval_views_for_user(&mut tx, identity.user_id, statuses).await?;
    tx.commit().await?;
    Ok(Json(ApprovalList {
        approvals: views
            .iter()
            .filter(|v| shown_to(&viewer, &v.approval, v.requested_for_unit_id))
            .map(|v| {
                let decides = can_decide(&viewer, &v.approval, v.requested_for_unit_id);
                Approval::from_view(v, decides)
            })
            .collect::<anyhow::Result<_>>()?,
    }))
}

async fn decide(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<ApprovalVerdict>,
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
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let decided = get_approval_view(&mut tx, result.approval.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("approval decided then gone"))?;
    tx.commit().await?;
    Ok(Json(ApprovalDecided {
        approval: Approval::from_view(&decided, false)?,
        resumed: result.resumed,
    }))
}
