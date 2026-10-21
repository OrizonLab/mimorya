//! Runs executed on behalf of the signed-in user, with their trace.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_db::repositories::approval_views::list_approval_views_for_run;
use mimorya_db::repositories::run_events::list_run_events;
use mimorya_db::repositories::runs::{get_run, list_runs};
use mimorya_db::repositories::tool_invocations::list_tool_invocations;

use crate::app::AppState;
use crate::contract::{Approval, RunDetail, RunList, RunListItem, RunView, ToolInvocation};
use crate::http::{ApiResult, PathId};
use crate::plugins::Auth;
use crate::services::runs::can_decide;

const RUN_PAGE: i64 = 50;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/runs", get(list))
        .route("/runs/{id}", get(detail))
}

async fn list(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<RunList>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_runs(&mut tx, identity.user_id, RUN_PAGE).await?;
    tx.commit().await?;
    let runs = rows
        .iter()
        .map(RunListItem::from_row)
        .collect::<anyhow::Result<_>>()?;
    Ok(Json(RunList { runs }))
}

async fn detail(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<RunDetail>> {
    let viewer = state.access.context(identity).await?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let run = get_run(&mut tx, id)
        .await?
        .filter(|r| r.on_behalf_of_user_id == identity.user_id)
        .ok_or_else(|| DomainError::not_found("Exécution introuvable"))?;
    let tools = list_tool_invocations(&mut tx, id).await?;
    let approvals = list_approval_views_for_run(&mut tx, id).await?;
    let events = list_run_events(&mut tx, id, 0).await?;
    tx.commit().await?;

    Ok(Json(RunDetail {
        run: RunView::from_row(&run)?,
        tools: tools
            .iter()
            .map(ToolInvocation::from_row)
            .collect::<anyhow::Result<_>>()?,
        approvals: approvals
            .iter()
            .map(|v| Approval::from_view(v, can_decide(&viewer, &v.approval)))
            .collect::<anyhow::Result<_>>()?,
        events: events.iter().map(Into::into).collect(),
    }))
}
