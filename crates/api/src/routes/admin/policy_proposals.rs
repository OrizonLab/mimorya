//! Allow policies proposed and decided (`policy:manage`, company-wide,
//! ADR-168): the listing, a proposal, and what another administrator, or
//! its author, decides of it.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use mimorya_db::repositories::policy_proposals::list_proposals;

use crate::app::AppState;
use crate::contract::admin::{NewPolicyProposal, PolicyProposalView, PolicyProposals};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{
    ProposalInput, approve_policy_proposal, propose_policy, reject_policy_proposal,
    withdraw_policy_proposal,
};
use crate::services::permission_catalog::permission_catalog;

const PERMISSION: &str = "policy:manage";

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<PolicyProposals>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_proposals(&mut tx).await?;
    tx.commit().await?;
    let now = Utc::now();
    let proposals = rows
        .iter()
        .map(|row| PolicyProposalView::from_row(row, identity.user_id, now))
        .collect::<anyhow::Result<_>>()?;
    Ok(Json(PolicyProposals { proposals }))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewPolicyProposal>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let input = ProposalInput {
        policy_id: body.policy_id,
        description: body.description,
        permissions: body.permissions,
        condition: body.condition,
        enabled: body.enabled.unwrap_or(true),
    };
    let id = propose_policy(&state.db, &access, &permission_catalog(), input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn approve(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let catalog = permission_catalog();
    approve_policy_proposal(&state.db, &state.access, &access, &catalog, id).await?;
    Ok(Json(Done::OK))
}

pub async fn reject(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    reject_policy_proposal(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}

pub async fn withdraw(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    withdraw_policy_proposal(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}
