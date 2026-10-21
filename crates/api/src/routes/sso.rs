//! Signing in through the company's identity provider (ADR-063): whether
//! an address may, the start towards the provider, and its return, which
//! sends the browser to the application signed in, or back to the sign-in
//! page with a generic message (the reason is audited).

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;

use crate::app::AppState;
use crate::auth::cookies::{cookie_value, session_cookie};
use crate::contract::{SsoDiscovery, SsoLookup, SsoProvider, SsoStart};
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::RequestContext;
use crate::services::sso::{
    ATTEMPT_COOKIE, Return, SignInFailure, attempt_cookie, cleared_attempt_cookie,
};

/// Where the browser lands, signed in or refused.
const SIGNED_IN: &str = "/chat";
const REFUSED: &str = "/login?sso=echec";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/sso/discover", post(discover))
        .route("/auth/sso/start", post(start))
        .route("/auth/sso/callback", get(callback))
}

/// Whether this client may ask about one more address, by the discovery or
/// the start: each tells whether it has an account tied to a provider.
fn asks_about(state: &AppState, context: &RequestContext, email: &str) -> bool {
    state
        .limits
        .sso_addresses
        .allow(&context.client_key(), &email.trim().to_lowercase())
}

/// The answer depends only on the account's company, never on whether an
/// account without a provider exists. It still says whether an address has
/// an account tied to a provider: so many addresses a minute, as logins.
async fn discover(
    State(state): State<AppState>,
    context: RequestContext,
    Body(body): Body<SsoLookup>,
) -> ApiResult<Json<SsoDiscovery>> {
    if !asks_about(&state, &context, &body.email) {
        return Err(ApiError::RateLimited);
    }
    let provider = state.sso.discover(&body.email).await?;
    Ok(Json(SsoDiscovery {
        provider: provider.map(|p| SsoProvider {
            kind: p.kind,
            name: p.name,
            required: p.required,
        }),
    }))
}

async fn start(
    State(state): State<AppState>,
    context: RequestContext,
    Body(body): Body<SsoLookup>,
) -> ApiResult<impl IntoResponse> {
    // Its answer tells what the discovery's does: the same addresses count.
    if !asks_about(&state, &context, &body.email)
        || !state.limits.sso_sign_in.allow(&context.client_key())
    {
        return Err(ApiError::RateLimited);
    }
    let started = state.sso.start(&body.email).await?;
    let cookie = attempt_cookie(&started.attempt, state.config.secure_cookies);
    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(SsoStart {
            redirect_url: started.redirect_url,
        }),
    ))
}

/// A top-level navigation from the provider: answered by a redirection,
/// the attempt's cookie cleared either way. Counted with the start: a
/// sealed attempt stays valid for its ten minutes, and each return with it
/// writes to the audit, or calls the provider.
async fn callback(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    back: Result<Query<Return>, QueryRejection>,
) -> Response {
    let secure = state.config.secure_cookies;
    let cleared = (SET_COOKIE, cleared_attempt_cookie(secure));
    let outcome = if state.limits.sso_sign_in.allow(&context.client_key()) {
        // An address that does not read as a return reads as an empty one.
        let back = back.map(|Query(back)| back).unwrap_or_default();
        let cookie = cookie_value(&headers, ATTEMPT_COOKIE);
        state
            .sso
            .finish(back, cookie.as_deref(), &context, &headers)
            .await
    } else {
        // Neither audited nor sent on: what floods is turned away first.
        Err(SignInFailure {
            tenant_id: None,
            provider_id: None,
            user_id: None,
            reason: "Trop de tentatives".into(),
        })
    };
    match outcome {
        Ok(token) => (
            StatusCode::SEE_OTHER,
            AppendHeaders([
                (LOCATION, SIGNED_IN.to_string()),
                cleared,
                (SET_COOKIE, session_cookie(&token, secure)),
            ]),
        )
            .into_response(),
        Err(failure) => {
            record_refusal(&state, &context, failure).await;
            (
                StatusCode::SEE_OTHER,
                AppendHeaders([(LOCATION, REFUSED.to_string()), cleared]),
            )
                .into_response()
        }
    }
}

/// In the company the attempt was for, when known; never a token nor a code.
async fn record_refusal(state: &AppState, context: &RequestContext, failure: SignInFailure) {
    let Some(tenant_id) = failure.tenant_id else {
        tracing::info!(reason = %failure.reason, "Single sign-on refused");
        return;
    };
    let mut entry = AuditEntry::new(tenant_id, "user", "auth.login", "denied")
        .reason(failure.reason.chars().take(300).collect::<String>())
        .metadata(json!({ "method": "sso", "providerId": failure.provider_id }))
        .ip(context.ip_string());
    if let Some(user_id) = failure.user_id {
        entry = entry.by_user(user_id);
    }
    entry.request_id = Some(context.id.to_string());
    let written = async {
        let mut tx = state.db.begin(tenant_id).await?;
        append_audit(&mut tx, entry).await?;
        tx.commit().await
    };
    if let Err(error) = written.await {
        tracing::warn!(error = %error, "Failed to audit a refused single sign-on");
    }
}
