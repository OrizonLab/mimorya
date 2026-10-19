//! Invitation links, before sign-in (ADR-027): what a link opens, then the
//! password its holder chooses, which opens a session. The token travels in
//! the request body, never in a URL; both routes are limited per client
//! like logins.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use validator::Validate;

use crate::app::AppState;
use crate::auth::cookies::session_cookie;
use crate::auth::credentials::MIN_PASSWORD_CHARS;
use crate::contract::{Done, InvitationDetails};
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::RequestContext;
use crate::services::invitations::{accept_invitation, inspect_invitation};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/invitation", post(inspect))
        .route("/auth/invitation/accept", post(accept))
}

#[derive(Deserialize, Validate)]
struct TokenBody {
    #[validate(length(min = 1, max = 200))]
    token: String,
}

#[derive(Deserialize, Validate)]
struct AcceptBody {
    #[validate(length(min = 1, max = 200))]
    token: String,
    #[validate(length(min = "MIN_PASSWORD_CHARS", max = 512))]
    password: String,
}

async fn inspect(
    State(state): State<AppState>,
    context: RequestContext,
    Body(body): Body<TokenBody>,
) -> ApiResult<Json<InvitationDetails>> {
    if !state.limits.login.allow(&context.client_key()) {
        return Err(ApiError::RateLimited);
    }
    let invitation = inspect_invitation(&state.db, &body.token).await?;
    Ok(Json((&invitation).into()))
}

async fn accept(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    Body(body): Body<AcceptBody>,
) -> ApiResult<impl IntoResponse> {
    if !state.limits.login.allow(&context.client_key()) {
        return Err(ApiError::RateLimited);
    }
    let session =
        accept_invitation(&state.db, &body.token, body.password, &context, &headers).await?;
    let cookie = session_cookie(&session, state.config.secure_cookies);
    Ok((AppendHeaders([(SET_COOKIE, cookie)]), Json(Done::OK)))
}
