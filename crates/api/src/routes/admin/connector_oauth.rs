//! OAuth on an MCP server (`connector:manage`, company-wide, ADR-171):
//! declared, where the discovery finds it is authorized, and withdrawn; who
//! authorizes it, the company or each person, changed (ADR-174); its
//! authorization started towards its authorization server and withdrawn;
//! the return, a top-level navigation in the administrator's session, which
//! sends the browser back to the administration saying only whether it was
//! authorized (the reason is audited); a person's own return, at the same
//! address, sent back to their account (ADR-174). The client's secret and the tokens
//! are write-only.

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{AppendHeaders, IntoResponse, Response};
use mimorya_connectors::mcp::oauth::{ClientAuth, OAuthHolder};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};

use super::connectors::{admin, secrets};
use crate::app::AppState;
use crate::auth::cookies::cookie_value;
use crate::contract::Done;
use crate::contract::admin::{
    McpOAuth, McpOAuthDeclaration, McpOAuthHolder, OAuthAuthorizationStart,
};
use crate::http::{ApiError, ApiResult, Body, PathId};
use crate::plugins::{Auth, RequestContext};
use crate::services::AccessContext;
use crate::services::admin::{
    AUTHORIZATION_COOKIE, AuthorizationFailure, OAuthInput, OAuthReturn, PERSON_AUTHORIZED,
    attempt_holder, authorization_cookie, cleared_authorization_cookie, declare_oauth,
    finish_authorization, finish_person_authorization, oauth_redirect_uri, remove_authorization,
    remove_oauth, set_oauth_holder, start_authorization,
};
use crate::services::permission_catalog::permission_catalog;

/// Where the browser lands after the authorization server.
const AUTHORIZED: &str = "/admin?autorisation=ok";
const FAILED: &str = "/admin?autorisation=echec";
/// Where a person's own authorization lands (ADR-174): their account.
const PERSON_AUTHORIZED_BACK: &str = "/account?serveur=ok";
const PERSON_FAILED_BACK: &str = "/account?serveur=echec";
/// A person's, refused for authorizing with another's account (ADR-175).
const PERSON_OTHER_ACCOUNT_BACK: &str = "/account?serveur=autre-compte";
/// Without a session, nothing can be authorized: the sign-in page.
const SIGNED_OUT: &str = "/login";

pub async fn declare(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<McpOAuthDeclaration>,
) -> ApiResult<Json<McpOAuth>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let input = OAuthInput {
        issuer: body.issuer.filter(|issuer| !issuer.is_empty()),
        register: body.register.unwrap_or(false),
        client_id: body.client_id.filter(|id| !id.is_empty()),
        client_secret: body.client_secret.filter(|secret| !secret.is_empty()),
        scopes: body.scopes,
        redirect_uri: oauth_redirect_uri(&state.config.web_origin),
        holder: body.per_person.map(holder),
        verify_identity: body.verify_identity,
        replaces: body.replaces,
    };
    let declared = declare_oauth(&admin(&state, &store, &catalog), &access, id, input).await?;
    // Its secret, typed or granted, stored if it proves itself with one; no
    // authorization yet, the company's nor anyone's.
    let has_secret = declared.client_auth != ClientAuth::None;
    Ok(Json(McpOAuth::new(declared, has_secret, None, 0)))
}

/// Who authorizes the server, changed in place (ADR-174).
pub async fn set_holder(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<McpOAuthHolder>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let admin = admin(&state, &store, &catalog);
    set_oauth_holder(&admin, &access, id, holder(body.per_person)).await?;
    Ok(Json(Done::OK))
}

fn holder(per_person: bool) -> OAuthHolder {
    if per_person {
        OAuthHolder::Person
    } else {
        OAuthHolder::Company
    }
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    remove_oauth(&admin(&state, &store, &catalog), &access, id).await?;
    Ok(Json(Done::OK))
}

/// Counted with the sign-ins through a provider: it leads to one.
pub async fn start(
    State(state): State<AppState>,
    context: RequestContext,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<impl IntoResponse> {
    let client = context.client_key();
    if !state.limits.sso_sign_in.allow(&client).await? {
        return Err(ApiError::RateLimited);
    }
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let admin = admin(&state, &store, &catalog);
    let started = start_authorization(&admin, &access, id, &state.config.web_origin).await?;
    let cookie = authorization_cookie(&started.attempt, state.config.secure_cookies);
    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(OAuthAuthorizationStart {
            redirect_url: started.redirect_url,
        }),
    ))
}

pub async fn remove_authorized(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    remove_authorization(&admin(&state, &store, &catalog), &access, id).await?;
    Ok(Json(Done::OK))
}

/// A top-level navigation from the authorization server: answered by a
/// redirection, the attempt's cookie cleared either way. Counted with the
/// start: each return with a sealed attempt calls the server.
pub async fn callback(
    State(state): State<AppState>,
    context: RequestContext,
    signed_in: Result<Auth, ApiError>,
    headers: HeaderMap,
    back: Result<Query<OAuthReturn>, QueryRejection>,
) -> Response {
    let cleared = (
        SET_COOKIE,
        cleared_authorization_cookie(state.config.secure_cookies),
    );
    let away = |to: &str| {
        (
            StatusCode::SEE_OTHER,
            AppendHeaders([(LOCATION, to.to_string()), cleared.clone()]),
        )
            .into_response()
    };
    let Ok(Auth(identity)) = signed_in else {
        return away(SIGNED_OUT);
    };
    let Ok(access) = state.access.context(identity).await else {
        return away(SIGNED_OUT);
    };
    let cookie = cookie_value(&headers, AUTHORIZATION_COOKIE);
    let (store, catalog) = (secrets(&state), permission_catalog());
    let admin = admin(&state, &store, &catalog);
    // A person's own attempt comes back here too (ADR-174), finished as
    // theirs and sent back to their account (`attempt_holder`).
    let person = attempt_holder(&admin, &access, cookie.as_deref()) == OAuthHolder::Person;
    let allowed = state
        .limits
        .sso_sign_in
        .allow(&context.client_key())
        .await
        .unwrap_or(false);
    let outcome = if allowed {
        // An address that does not read as a return reads as an empty one.
        let back = back.map(|Query(back)| back).unwrap_or_default();
        let origin = &state.config.web_origin;
        let cookie = cookie.as_deref();
        if person {
            finish_person_authorization(&admin, &access, back, cookie, origin).await
        } else {
            finish_authorization(&admin, &access, back, cookie, origin).await
        }
    } else {
        Err(AuthorizationFailure {
            connector_id: None,
            reason: "Trop de tentatives".into(),
            other_account: false,
        })
    };
    let (action, authorized, failed) = if person {
        (
            PERSON_AUTHORIZED,
            PERSON_AUTHORIZED_BACK,
            PERSON_FAILED_BACK,
        )
    } else {
        ("connector.oauth_authorized", AUTHORIZED, FAILED)
    };
    match outcome {
        Ok(_) => away(authorized),
        Err(failure) => {
            let back = if person && failure.other_account {
                PERSON_OTHER_ACCOUNT_BACK
            } else {
                failed
            };
            record_refusal(&state, &context, &access, action, failure).await;
            away(back)
        }
    }
}

/// Never a token, a code nor a secret.
async fn record_refusal(
    state: &AppState,
    context: &RequestContext,
    access: &AccessContext,
    action: &'static str,
    failure: AuthorizationFailure,
) {
    let tenant_id = access.subject.tenant_id;
    let mut entry = AuditEntry::new(tenant_id, "user", action, "denied")
        .by_user(access.subject.user_id)
        .reason(failure.reason.chars().take(300).collect::<String>())
        .ip(context.ip_string());
    if let Some(connector_id) = failure.connector_id {
        entry = entry.resource("connector", connector_id);
    }
    entry.request_id = Some(context.id.to_string());
    let written = async {
        let mut tx = state.db.begin(tenant_id).await?;
        append_audit(&mut tx, entry).await?;
        tx.commit().await
    };
    if let Err(error) = written.await {
        tracing::warn!(error = %error, "Failed to audit a refused connector authorization");
    }
}
