//! A SAML provider's side of the sign-in (ADR-068): a company's own
//! metadata, which its provider imports, and the address it posts its
//! response to, from its own site. That post carries no header of this
//! application: the CSRF check lets it through, the sealed attempt in its
//! own cookie and the signed response bind it instead. It answers by a
//! redirection, signed in or back to the sign-in page, the attempt cleared.

use axum::Form;
use axum::Router;
use axum::extract::State;
use axum::extract::rejection::FormRejection;
use axum::http::header::{CONTENT_TYPE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::routing::{get, post};

use super::sso::{REFUSED, SIGNED_IN, record_refusal};
use crate::app::AppState;
use crate::auth::cookies::{cookie_value, session_cookie};
use crate::http::{ApiResult, PathId};
use crate::plugins::RequestContext;
use crate::services::sso::{
    SAML_ATTEMPT_COOKIE, SamlReturn, SignInFailure, cleared_saml_attempt_cookie,
};

/// Where the provider posts its response, as the router sees it: the
/// route below, exempted from the CSRF header (the tests post without it).
pub const ACS_ROUTE: &str = "/auth/saml/acs";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/saml/{id}/metadata", get(metadata))
        .route("/auth/saml/acs", post(acs))
}

/// Derived from its address alone: the company's entity ID is public.
async fn metadata(State(state): State<AppState>, PathId(tenant_id): PathId) -> ApiResult<Response> {
    let xml = state.sso.saml_metadata(tenant_id)?;
    Ok(([(CONTENT_TYPE, "application/samlmetadata+xml")], xml).into_response())
}

/// Counted with the sign-ins through a provider, as an OpenID Connect
/// return is.
async fn acs(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    back: Result<Form<SamlReturn>, FormRejection>,
) -> Response {
    let cleared = (SET_COOKIE, cleared_saml_attempt_cookie());
    let outcome = if state.limits.sso_sign_in.allow(&context.client_key()) {
        // A body that does not read as a response reads as an empty one.
        let back = back.map(|Form(back)| back).unwrap_or_default();
        let cookie = cookie_value(&headers, SAML_ATTEMPT_COOKIE);
        state
            .sso
            .finish_saml(back, cookie.as_deref(), &context, &headers)
            .await
    } else {
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
                (
                    SET_COOKIE,
                    session_cookie(&token, state.config.secure_cookies),
                ),
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
