//! The second factor at sign-in (ADR-200, ADR-201): what a password checked
//! answers, what the pending sign-in waits for, the code that completes it,
//! and the enrolment it waits for when the company requires a second
//! factor the account has not. The challenge travels in its own cookie,
//! sent to these routes alone; every step that checks or changes something
//! is limited per client like logins, and every code refused counts
//! against the account.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_core::second_factor::SecondFactorCode;
use mimorya_db::repositories::login_challenges::ChallengePurpose;

use super::second_factor::{NOT_SIX_DIGITS, read_code};
use crate::app::AppState;
use crate::auth::cookies::{cookie_value, session_cookie};
use crate::contract::{RecoveryCodes, SecondFactorCodeInput, SecondFactorEnrolment, SignIn};
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::RequestContext;
use crate::services::second_factor::{
    CHALLENGE_COOKIE, Completion, EXPIRED, Opened, challenge_cookie, challenge_purpose,
    cleared_challenge_cookie, complete_challenge, complete_sign_in_enrolment,
    start_sign_in_enrolment,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/second-factor", get(waiting).post(sign_in))
        .route("/auth/second-factor/enrolment", post(enrol))
        .route("/auth/second-factor/enrolment/confirmation", post(confirm))
}

/// What a sign-in answers once the password is checked: the session's
/// cookie, or the challenge's when the second factor is asked for next, to
/// prove or to enrol.
pub(super) fn signed_in(opened: Opened, secure: bool) -> impl IntoResponse {
    let (cookie, next) = match opened {
        Opened::Session(token) => (session_cookie(&token, secure), SignIn::SIGNED_IN),
        Opened::Challenge(token, purpose) => (challenge_cookie(&token, secure), next(purpose)),
    };
    (AppendHeaders([(SET_COOKIE, cookie)]), Json(next))
}

fn next(purpose: ChallengePurpose) -> SignIn {
    match purpose {
        ChallengePurpose::Verify => SignIn::SECOND_FACTOR,
        ChallengePurpose::Enrol => SignIn::SECOND_FACTOR_ENROLMENT,
    }
}

/// The challenge the browser holds.
fn held_challenge(headers: &HeaderMap) -> ApiResult<String> {
    cookie_value(headers, CHALLENGE_COOKIE)
        .ok_or_else(|| DomainError::unauthenticated(EXPIRED).into())
}

/// The challenge the browser holds, for a step that checks or changes
/// something: after the per-client limit of logins.
async fn challenge(
    state: &AppState,
    context: &RequestContext,
    headers: &HeaderMap,
) -> ApiResult<String> {
    if !state.limits.login.allow(&context.client_key()).await? {
        return Err(ApiError::RateLimited);
    }
    held_challenge(headers)
}

fn completion<'a>(
    state: &'a AppState,
    context: &'a RequestContext,
    headers: &'a HeaderMap,
) -> Completion<'a> {
    Completion {
        db: &state.db,
        cipher: state.config.secrets.as_deref(),
        refusals: &state.limits.second_factor_failures,
        context,
        headers,
    }
}

/// What the pending sign-in waits for: a read that checks nothing, left out
/// of the logins' limit (the global one still counts it), so that a page
/// reloaded spends none of the steps a sign-in takes.
async fn waiting(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<SignIn>> {
    let token = held_challenge(&headers)?;
    let purpose = challenge_purpose(&state.db, &token).await?;
    Ok(Json(next(purpose)))
}

/// Completes a sign-in waiting for its second factor: the session opens.
async fn sign_in(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    Body(body): Body<SecondFactorCodeInput>,
) -> ApiResult<impl IntoResponse> {
    let token = challenge(&state, &context, &headers).await?;
    let code = read_code(&body.code)?;
    let completion = completion(&state, &context, &headers);
    let session = complete_challenge(completion, &token, &code).await?;
    let secure = state.config.secure_cookies;
    Ok((
        AppendHeaders([
            (SET_COOKIE, session_cookie(&session, secure)),
            (SET_COOKIE, cleared_challenge_cookie(secure)),
        ]),
        Json(SignIn::SIGNED_IN),
    ))
}

/// Starts the enrolment a sign-in waits for.
async fn enrol(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
) -> ApiResult<Json<SecondFactorEnrolment>> {
    let token = challenge(&state, &context, &headers).await?;
    let enrolment =
        start_sign_in_enrolment(&state.db, state.config.secrets.as_deref(), &token).await?;
    Ok(Json(SecondFactorEnrolment {
        secret: enrolment.secret,
        uri: enrolment.uri,
    }))
}

/// Turns that enrolment on with its first code: the recovery codes, shown
/// this once, and the session.
async fn confirm(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    Body(body): Body<SecondFactorCodeInput>,
) -> ApiResult<impl IntoResponse> {
    let token = challenge(&state, &context, &headers).await?;
    let Some(SecondFactorCode::Totp(code)) = SecondFactorCode::read(&body.code) else {
        return Err(DomainError::validation(NOT_SIX_DIGITS).into());
    };
    let completion = completion(&state, &context, &headers);
    let enrolled = complete_sign_in_enrolment(completion, &token, code).await?;
    let secure = state.config.secure_cookies;
    Ok((
        AppendHeaders([
            (SET_COOKIE, session_cookie(&enrolled.session, secure)),
            (SET_COOKIE, cleared_challenge_cookie(secure)),
        ]),
        Json(RecoveryCodes {
            codes: enrolled.codes,
        }),
    ))
}
