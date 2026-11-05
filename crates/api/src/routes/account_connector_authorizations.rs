//! The MCP servers the signed-in person authorizes for themself (ADR-174):
//! those they may authorize and their own authorization of each; the start
//! towards the authorization server, whose return arrives at the address
//! the company's does and sends the browser back to their account; and the
//! withdrawal. Never anyone else's, never a token.

use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};

use super::admin::connectors::{admin, secrets};
use crate::app::AppState;
use crate::contract::admin::OAuthAuthorizationStart;
use crate::contract::{Done, PersonAuthorizationView, PersonAuthorizations};
use crate::http::{ApiError, ApiResult, PathId};
use crate::plugins::{Auth, RequestContext};
use crate::services::admin::{
    authorization_cookie, list_person_servers, remove_person_authorization,
    start_person_authorization,
};
use crate::services::permission_catalog::permission_catalog;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/account/connector-authorizations", get(list))
        .route(
            "/account/connector-authorizations/{id}",
            post(start).delete(remove),
        )
}

async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<PersonAuthorizations>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let servers = list_person_servers(&admin(&state, &store, &catalog), &access).await?;
    let servers = servers
        .into_iter()
        .map(|server| PersonAuthorizationView {
            connector_id: server.connector_id,
            name: server.name,
            available: server.available,
            authorization: server.authorization.map(Into::into),
        })
        .collect();
    Ok(Json(PersonAuthorizations { servers }))
}

/// Counted with the sign-ins through a provider, as the company's start.
async fn start(
    State(state): State<AppState>,
    context: RequestContext,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<impl IntoResponse> {
    if !state
        .limits
        .sso_sign_in
        .allow(&context.client_key())
        .await?
    {
        return Err(ApiError::RateLimited);
    }
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let admin = admin(&state, &store, &catalog);
    let origin = &state.config.web_origin;
    let started = start_person_authorization(&admin, &access, id, origin).await?;
    let cookie = authorization_cookie(&started.attempt, state.config.secure_cookies);
    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(OAuthAuthorizationStart {
            redirect_url: started.redirect_url,
        }),
    ))
}

async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    remove_person_authorization(&admin(&state, &store, &catalog), &access, id).await?;
    Ok(Json(Done::OK))
}
