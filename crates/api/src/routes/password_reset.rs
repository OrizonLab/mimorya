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
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

use crate::app::AppState;
use crate::auth::cookies::session_cookie;
use crate::auth::credentials::MIN_PASSWORD_CHARS;
use crate::http::{ApiError, ApiResult, Body, iso, trimmed};
use crate::plugins::RequestContext;
use crate::services::password_reset::{
    ResetRequest, ResetSender, complete_reset, inspect_reset, send_reset_link,
};

const NO_RELAY: &str = "La réinitialisation par e-mail n’est pas disponible : demandez à votre \
     administrateur de réinitialiser votre mot de passe";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/password-reset", post(request))
        .route("/auth/password-reset/link", post(inspect))
        .route("/auth/password-reset/complete", post(complete))
}

#[derive(Deserialize, Validate)]
struct RequestBody {
    #[serde(deserialize_with = "trimmed")]
    #[validate(email, length(max = 254))]
    email: String,
}

#[derive(Deserialize, Validate)]
struct TokenBody {
    #[validate(length(min = 1, max = 200))]
    token: String,
}

#[derive(Deserialize, Validate)]
struct CompleteBody {
    #[validate(length(min = 1, max = 200))]
    token: String,
    #[validate(length(min = "MIN_PASSWORD_CHARS", max = 512))]
    password: String,
}

async fn request(
    State(state): State<AppState>,
    context: RequestContext,
    Body(body): Body<RequestBody>,
) -> ApiResult<impl IntoResponse> {
    let Some(mailer) = state.mailer.clone() else {
        return Err(DomainError::unavailable(NO_RELAY).into());
    };
    if !state.limits.login.allow(&context.client_key()) {
        return Err(ApiError::RateLimited);
    }
    // Per address as well, known or not: nothing tells them apart, and no
    // mailbox is flooded with links.
    let email = body.email.to_lowercase();
    if !state.limits.reset_requests.allow(&email) {
        return Err(ApiError::RateLimited);
    }
    let sender = ResetSender {
        db: state.db.clone(),
        mailer,
        web_origin: state.config.web_origin.clone(),
        product: state.config.assistant_name.clone(),
    };
    let request = ResetRequest {
        email,
        ip: context.ip_string(),
        request_id: context.id.to_string(),
    };
    state.background.spawn(send_reset_link(sender, request));
    Ok((StatusCode::ACCEPTED, Json(json!({ "ok": true }))))
}

async fn inspect(
    State(state): State<AppState>,
    context: RequestContext,
    Body(body): Body<TokenBody>,
) -> ApiResult<Json<Value>> {
    if !state.limits.login.allow(&context.client_key()) {
        return Err(ApiError::RateLimited);
    }
    let reset = inspect_reset(&state.db, &body.token).await?;
    Ok(Json(json!({
        "email": reset.email,
        "expiresAt": iso(&reset.expires_at),
    })))
}

async fn complete(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    Body(body): Body<CompleteBody>,
) -> ApiResult<impl IntoResponse> {
    if !state.limits.login.allow(&context.client_key()) {
        return Err(ApiError::RateLimited);
    }
    let session = complete_reset(&state.db, &body.token, body.password, &context, &headers).await?;
    let cookie = session_cookie(&session, state.config.secure_cookies);
    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(json!({ "ok": true })),
    ))
}
