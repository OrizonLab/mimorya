//! The risks whose actions the company's managers validate (ADR-059):
//! read and replaced by those who manage its access policies, who also
//! learn whether managers are e-mailed (ADR-060).

use axum::Json;
use axum::extract::State;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{ApprovalRoutingUpdate, ApprovalRoutingView};
use crate::contract::stored_text;
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::admin::{manager_risks, set_manager_risks};

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<ApprovalRoutingView>> {
    let access = state.access.context(identity).await?;
    let risks = manager_risks(&state.db, &access).await?;
    Ok(Json(ApprovalRoutingView {
        manager_risks: risks
            .iter()
            .map(|risk| stored_text("manager risk", risk))
            .collect::<anyhow::Result<_>>()?,
        notifies_by_email: state.mailer.is_some(),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<ApprovalRoutingUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let risks: Vec<_> = body.manager_risks.into_iter().map(Into::into).collect();
    set_manager_risks(&state.db, &access, &risks).await?;
    Ok(Json(Done::OK))
}
