//! The second factor (ADR-200): one's own, from « Mon compte », and the
//! code that completes a sign-in waiting for it. Every code checked and
//! refused, at sign-in, at activation or later, counts against the account,
//! shared by every process; a slot is given back when the code is accepted
//! or none was checked.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_core::second_factor::SecondFactorCode;

use crate::app::AppState;
use crate::auth::cookies::{cookie_value, session_cookie};
use crate::contract::{
    Done, RecoveryCodes, SecondFactorCodeInput, SecondFactorEnrolment,
    SecondFactorEnrolmentRequest, SecondFactorStatus, SignIn,
};
use crate::http::{ApiError, ApiResult, Body, iso};
use crate::plugins::{Auth, RequestContext};
use crate::secrets::SecretStore;
use crate::services::second_factor::{
    CHALLENGE_COOKIE, Completion, EXPIRED, Opened, Refusal, challenge_cookie,
    cleared_challenge_cookie, complete_challenge, confirm_enrolment, disable_second_factor,
    own_second_factor, renew_recovery_codes, start_enrolment,
};

const NOT_A_CODE: &str = "Saisissez les six chiffres de votre application ou l’un de vos codes \
     de secours";
const NOT_SIX_DIGITS: &str = "Saisissez les six chiffres affichés par votre application";
const WRONG_FIRST_CODE: &str = "Code incorrect : vérifiez que l’heure de votre téléphone est \
     juste et saisissez le code affiché maintenant";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/account/second-factor", get(status))
        .route("/account/second-factor/enrolment", post(enrol))
        .route("/account/second-factor/confirmation", post(confirm))
        .route("/account/second-factor/recovery-codes", post(renew_codes))
        .route("/account/second-factor/deactivation", post(deactivate))
        .route("/auth/second-factor", post(sign_in))
}

/// What a sign-in answers once the password is checked: the session's
/// cookie, or the challenge's when the second factor is asked for next.
pub(super) fn signed_in(opened: Opened, secure: bool) -> impl IntoResponse {
    let (cookie, next) = match opened {
        Opened::Session(token) => (session_cookie(&token, secure), SignIn::SIGNED_IN),
        Opened::Challenge(token) => (challenge_cookie(&token, secure), SignIn::SECOND_FACTOR),
    };
    (AppendHeaders([(SET_COOKIE, cookie)]), Json(next))
}

fn secrets(state: &AppState) -> SecretStore {
    SecretStore::new(state.config.secrets.clone())
}

fn read_code(input: &str) -> Result<SecondFactorCode, ApiError> {
    SecondFactorCode::read(input).ok_or_else(|| DomainError::validation(NOT_A_CODE).into())
}

async fn status(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<SecondFactorStatus>> {
    let access = state.access.context(identity).await?;
    let own = own_second_factor(&state.db, &secrets(&state), &access).await?;
    Ok(Json(SecondFactorStatus {
        applicable: own.applicable,
        available: own.available,
        enabled: own.enabled_at.is_some(),
        enabled_at: own.enabled_at.as_ref().map(iso),
        recovery_codes_left: own.recovery_codes_left,
    }))
}

async fn enrol(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<SecondFactorEnrolmentRequest>,
) -> ApiResult<Json<SecondFactorEnrolment>> {
    let access = state.access.context(identity).await?;
    // Guessing the password counts against the account like a login.
    let account_key = access.profile.email.to_lowercase();
    if !state.limits.failed_logins.allow(&account_key).await? {
        return Err(ApiError::RateLimited);
    }
    let started = start_enrolment(&state.db, &secrets(&state), &access, body.password).await;
    if !matches!(started, Ok(None)) {
        state.limits.failed_logins.release(&account_key).await;
    }
    let Some(enrolment) = started? else {
        return Err(DomainError::forbidden("Mot de passe incorrect").into());
    };
    Ok(Json(SecondFactorEnrolment {
        secret: enrolment.secret,
        uri: enrolment.uri,
    }))
}

async fn confirm(
    State(state): State<AppState>,
    context: RequestContext,
    Auth(identity): Auth,
    Body(body): Body<SecondFactorCodeInput>,
) -> ApiResult<Json<RecoveryCodes>> {
    let Some(SecondFactorCode::Totp(code)) = SecondFactorCode::read(&body.code) else {
        return Err(DomainError::validation(NOT_SIX_DIGITS).into());
    };
    let access = state.access.context(identity).await?;
    let account = access.subject.user_id.to_string();
    take_slot(&state, &account).await?;
    let confirmed = confirm_enrolment(
        &state.db,
        &secrets(&state),
        &access,
        identity.session_id,
        code,
        &context,
    )
    .await;
    // Only a wrong first code counts; a refusal before it (no enrolment)
    // checked none.
    if !matches!(confirmed, Ok(None)) {
        state.limits.second_factor_failures.release(&account).await;
    }
    let Some(codes) = confirmed? else {
        return Err(DomainError::forbidden(WRONG_FIRST_CODE).into());
    };
    Ok(Json(RecoveryCodes { codes }))
}

/// Takes a slot of the account's refusals before a code is checked.
async fn take_slot(state: &AppState, account: &str) -> Result<(), ApiError> {
    if state.limits.second_factor_failures.allow(account).await? {
        Ok(())
    } else {
        Err(ApiError::RateLimited)
    }
}

/// Gives the slot back unless a code was checked and refused.
async fn settle<T>(state: &AppState, account: &str, outcome: Result<T, Refusal>) -> ApiResult<T> {
    if outcome
        .as_ref()
        .err()
        .is_none_or(|refusal| !refusal.counts())
    {
        state.limits.second_factor_failures.release(account).await;
    }
    outcome.map_err(|refusal| refusal.error(false).into())
}

async fn renew_codes(
    State(state): State<AppState>,
    context: RequestContext,
    Auth(identity): Auth,
    Body(body): Body<SecondFactorCodeInput>,
) -> ApiResult<Json<RecoveryCodes>> {
    let code = read_code(&body.code)?;
    let access = state.access.context(identity).await?;
    let account = access.subject.user_id.to_string();
    take_slot(&state, &account).await?;
    let outcome =
        renew_recovery_codes(&state.db, &secrets(&state), &access, &code, &context).await?;
    let codes = settle(&state, &account, outcome).await?;
    Ok(Json(RecoveryCodes { codes }))
}

async fn deactivate(
    State(state): State<AppState>,
    context: RequestContext,
    Auth(identity): Auth,
    Body(body): Body<SecondFactorCodeInput>,
) -> ApiResult<Json<Done>> {
    let code = read_code(&body.code)?;
    let access = state.access.context(identity).await?;
    let account = access.subject.user_id.to_string();
    take_slot(&state, &account).await?;
    let outcome =
        disable_second_factor(&state.db, &secrets(&state), &access, &code, &context).await?;
    settle(&state, &account, outcome).await?;
    Ok(Json(Done::OK))
}

/// Completes a sign-in waiting for its second factor: the session opens.
async fn sign_in(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    Body(body): Body<SecondFactorCodeInput>,
) -> ApiResult<impl IntoResponse> {
    if !state.limits.login.allow(&context.client_key()).await? {
        return Err(ApiError::RateLimited);
    }
    let Some(token) = cookie_value(&headers, CHALLENGE_COOKIE) else {
        return Err(DomainError::unauthenticated(EXPIRED).into());
    };
    let code = read_code(&body.code)?;
    let cipher = state.config.secrets.as_deref();
    let completion = Completion {
        db: &state.db,
        cipher,
        refusals: &state.limits.second_factor_failures,
        context: &context,
        headers: &headers,
    };
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
