//! Invitation links, before sign-in (ADR-027): what a link opens, then the
//! password its holder chooses, which opens a session (or first the
//! enrolment of a second factor the company requires, ADR-201). The token
//! travels in the request body, never in a URL; both routes are limited per
//! client like logins.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};

use super::second_factor_sign_in::signed_in;
use crate::app::AppState;
use crate::contract::{InvitationDetails, LinkPassword, LinkToken};
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::RequestContext;
use crate::services::invitations::{accept_invitation, inspect_invitation};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/invitation", post(inspect))
        .route("/auth/invitation/accept", post(accept))
}

async fn inspect(
    State(state): State<AppState>,
    context: RequestContext,
    Body(body): Body<LinkToken>,
) -> ApiResult<Json<InvitationDetails>> {
    if !state.limits.login.allow(&context.client_key()).await? {
        return Err(ApiError::RateLimited);
    }
    let invitation = inspect_invitation(&state.db, &body.token).await?;
    Ok(Json((&invitation).into()))
}

async fn accept(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    Body(body): Body<LinkPassword>,
) -> ApiResult<impl IntoResponse> {
    if !state.limits.login.allow(&context.client_key()).await? {
        return Err(ApiError::RateLimited);
    }
    // A session, or the enrolment of a second factor the company requires
    // first (ADR-201).
    let opened = accept_invitation(
        &state.db,
        &state.access,
        &body.token,
        body.password,
        &context,
        &headers,
    )
    .await?;
    Ok(signed_in(opened, state.config.secure_cookies))
}
