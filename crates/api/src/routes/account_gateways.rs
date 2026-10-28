//! The conversation gateways of the signed-in person's company, and who they
//! are on each (ADR-125, ADR-126): listed for them; the link Slack gave them, its
//! code read from the page's fragment and sent in a body (never in an
//! address the servers log), shown then confirmed in their own session
//! (counted with the sign-ins through a provider: each confirmation asks
//! Slack); their identity removed.

use axum::extract::State;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;

use crate::app::AppState;
use crate::contract::{
    AccountGatewayView, AccountGateways, Done, GatewayIdentity, GatewayLinkCode, GatewayLinkPreview,
};
use crate::http::{ApiError, ApiResult, Body, PathId, iso};
use crate::plugins::{Auth, RequestContext};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/account/gateways", get(list))
        .route("/account/gateways/link/preview", post(preview))
        .route("/account/gateways/link", post(confirm))
        .route("/account/gateways/{id}", delete(remove))
}

async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<AccountGateways>> {
    let access = state.access.context(identity).await?;
    let gateways = state
        .gateways
        .gateways_of(&access)
        .await?
        .into_iter()
        .map(|of| AccountGatewayView {
            gateway_id: of.gateway.id,
            kind: of.gateway.kind,
            name: of.gateway.name,
            workspace_name: of.gateway.workspace_name,
            available: of.available,
            linked: of.identity.map(|identity| GatewayIdentity {
                external_id: identity.external_id,
                address: identity.address,
                linked_at: iso(&identity.created_at),
            }),
        })
        .collect();
    Ok(Json(AccountGateways { gateways }))
}

async fn preview(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<GatewayLinkCode>,
) -> ApiResult<Json<GatewayLinkPreview>> {
    let access = state.access.context(identity).await?;
    let preview = state
        .gateways
        .preview_link(&access, &body.code, Utc::now())
        .await?;
    Ok(Json(GatewayLinkPreview {
        gateway_id: preview.gateway.id,
        kind: preview.gateway.kind,
        name: preview.gateway.name,
        workspace_name: preview.gateway.workspace_name,
        external_id: preview.external_id,
        expires_at: iso(&preview.expires_at),
    }))
}

async fn confirm(
    State(state): State<AppState>,
    context: RequestContext,
    Auth(identity): Auth,
    Body(body): Body<GatewayLinkCode>,
) -> ApiResult<Json<Done>> {
    if !state
        .limits
        .sso_sign_in
        .allow(&context.client_key())
        .await?
    {
        return Err(ApiError::RateLimited);
    }
    let access = state.access.context(identity).await?;
    state
        .gateways
        .confirm_link(&access, &body.code, Utc::now())
        .await?;
    Ok(Json(Done::OK))
}

async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(gateway_id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    state.gateways.unlink(&access, gateway_id).await?;
    Ok(Json(Done::OK))
}
