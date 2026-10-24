//! Password reset links, before sign-in (ADR-028): asking for one, what a
//! link opens, then the new password, which opens a session. Asking always
//! gets the same answer: the link is issued in the background, and only if
//! the address designates an active account. The token travels in the
//! request body, never in a URL; every route is limited per client like
//! logins, and asking also per address.

use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::post;
use axum::{Json, Router};
use mimorya_core::DomainError;

use crate::app::AppState;
use crate::auth::cookies::session_cookie;
use crate::contract::{Done, LinkPassword, LinkToken, ResetDetails, ResetLinkRequest};
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::RequestContext;
use crate::services::password_reset::{
    ResetRequest, ResetSender, complete_reset, inspect_reset, send_requested_link,
};

const NO_RELAY: &str = "La réinitialisation par e-mail n’est pas disponible : demandez à votre \
     administrateur de réinitialiser votre mot de passe";

/// The window lasts an hour, and anyone may have used it up (ADR-029).
const TOO_MANY_LINKS: &str = "Trop de liens demandés pour cette adresse : réessayez dans une \
     heure, ou demandez à votre administrateur de vous en envoyer un";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/password-reset", post(request))
        .route("/auth/password-reset/link", post(inspect))
        .route("/auth/password-reset/complete", post(complete))
}

async fn request(
    State(state): State<AppState>,
    context: RequestContext,
    Body(body): Body<ResetLinkRequest>,
) -> ApiResult<impl IntoResponse> {
    let Some(mailer) = state.mailer.clone() else {
        return Err(DomainError::unavailable(NO_RELAY).into());
    };
    if !state.limits.login.allow(&context.client_key()).await? {
        return Err(ApiError::RateLimited);
    }
    // Per address as well, known or not: nothing tells them apart, and no
    // mailbox is flooded with links.
    let email = body.email.to_lowercase();
    if !state.limits.reset_requests.allow(&email).await? {
        return Err(ApiError::Throttled(TOO_MANY_LINKS));
    }
    let sender = ResetSender {
        db: state.db.clone(),
        access: state.access.clone(),
        mailer,
        web_origin: state.config.web_origin.clone(),
        product: state.config.assistant_name.clone(),
    };
    let request = ResetRequest {
        email,
        ip: context.ip_string(),
        request_id: context.id.to_string(),
    };
    state.background.spawn(send_requested_link(sender, request));
    Ok((StatusCode::ACCEPTED, Json(Done::OK)))
}

async fn inspect(
    State(state): State<AppState>,
    context: RequestContext,
    Body(body): Body<LinkToken>,
) -> ApiResult<Json<ResetDetails>> {
    if !state.limits.login.allow(&context.client_key()).await? {
        return Err(ApiError::RateLimited);
    }
    let reset = inspect_reset(&state.db, &body.token).await?;
    Ok(Json((&reset).into()))
}

async fn complete(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    Body(body): Body<LinkPassword>,
) -> ApiResult<impl IntoResponse> {
    if !state.limits.login.allow(&context.client_key()).await? {
        return Err(ApiError::RateLimited);
    }
    let session = complete_reset(
        &state.db,
        &state.access,
        &body.token,
        body.password,
        &context,
        &headers,
    )
    .await?;
    let cookie = session_cookie(&session, state.config.secure_cookies);
    Ok((AppendHeaders([(SET_COOKIE, cookie)]), Json(Done::OK)))
}
