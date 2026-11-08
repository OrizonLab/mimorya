//! Where the company's identity provider provisions its accounts through
//! SCIM 2.0 (ADR-183), before any session: its token, sent as
//! `Authorization: Bearer <token>`, is its only proof; a session cookie
//! proves nothing here. Every route of the prefix is the provider's alone:
//! a change it sends goes without the request header the browser's carry
//! (`plugins::csrf`). Answers and refusals are SCIM's
//! (`application/scim+json`), the token read first, even for a method or a
//! path the prefix does not serve.

use std::collections::HashMap;

use axum::Router;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use uuid::Uuid;

use crate::app::AppState;
use crate::contract::{ScimServiceProviderConfig, ScimUser, ScimUserList, scim_url};
use crate::http::bearer;
use crate::http::scim::{ScimBody, ScimJson, ScimResult};
use crate::services::scim::{
    ScimPage, authenticate, create_scim_user, list_scim_users, patch_scim_user, read_scim_user,
    remove_scim_user, replace_scim_user,
};

use super::admin::connectors::{admin, secrets};
use crate::http::scim::ScimError;
use crate::services::permission_catalog::permission_catalog;

/// What every SCIM route's path starts with.
pub const PREFIX: &str = "/scim/v2/";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/scim/v2/ServiceProviderConfig",
            get(config).fallback(not_allowed),
        )
        .route(
            "/scim/v2/Users",
            get(list).post(create).fallback(not_allowed),
        )
        .route(
            "/scim/v2/Users/{id}",
            get(read)
                .put(replace)
                .patch(patch)
                .delete(remove)
                .fallback(not_allowed),
        )
}

/// Where the provider reaches SCIM.
fn base(state: &AppState) -> String {
    scim_url(&state.config.web_origin)
}

async fn config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ScimResult<ScimJson<ScimServiceProviderConfig>> {
    // Read with a token, as the rest: nothing is said to anyone else.
    authenticate(&state.db, bearer(&headers)).await?;
    Ok(ScimJson::ok(ScimServiceProviderConfig::at(&base(&state))))
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> ScimResult<ScimJson<ScimUserList>> {
    // Read once the token is (`services::scim`).
    let page = ScimPage {
        filter: query.get("filter").cloned(),
        start_index: query.get("startIndex").cloned(),
        count: query.get("count").cloned(),
    };
    let list = list_scim_users(&state.db, bearer(&headers), page, &base(&state)).await?;
    Ok(ScimJson::ok(list))
}

/// The account a path names, if it names one: otherwise refused as absent,
/// once the token is.
fn named(path: Result<Path<String>, PathRejection>) -> Option<Uuid> {
    path.ok().and_then(|Path(id)| id.parse().ok())
}

async fn read(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
) -> ScimResult<ScimJson<ScimUser>> {
    let user = read_scim_user(&state.db, bearer(&headers), named(path), &base(&state)).await?;
    Ok(ScimJson::ok(user))
}

async fn replace(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    ScimBody(body): ScimBody,
) -> ScimResult<ScimJson<ScimUser>> {
    let (store, catalog) = (secrets(&state), permission_catalog());
    let admin = admin(&state, &store, &catalog);
    let (token, base) = (bearer(&headers), base(&state));
    let user = replace_scim_user(&admin, token, named(path), body, &base).await?;
    Ok(ScimJson::ok(user))
}

async fn patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    ScimBody(body): ScimBody,
) -> ScimResult<ScimJson<ScimUser>> {
    let (store, catalog) = (secrets(&state), permission_catalog());
    let admin = admin(&state, &store, &catalog);
    let (token, base) = (bearer(&headers), base(&state));
    let user = patch_scim_user(&admin, token, named(path), body, &base).await?;
    Ok(ScimJson::ok(user))
}

async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
) -> ScimResult<StatusCode> {
    let (store, catalog) = (secrets(&state), permission_catalog());
    let admin = admin(&state, &store, &catalog);
    remove_scim_user(&admin, bearer(&headers), named(path)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A method the path does not serve: refused once the token is.
async fn not_allowed(State(state): State<AppState>, headers: HeaderMap) -> ScimError {
    match authenticate(&state.db, bearer(&headers)).await {
        Ok(()) => ScimError::method_not_allowed(),
        Err(refused) => refused,
    }
}

/// A path under the prefix that serves nothing: refused once the token is.
pub async fn no_such_path(state: &AppState, headers: &HeaderMap) -> Response {
    match authenticate(&state.db, bearer(headers)).await {
        Ok(()) => ScimError::no_such_path().into_response(),
        Err(refused) => refused.into_response(),
    }
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    ScimBody(body): ScimBody,
) -> ScimResult<ScimJson<ScimUser>> {
    let user = create_scim_user(&state.db, bearer(&headers), body, &base(&state)).await?;
    let location = user.meta.location.clone();
    Ok(ScimJson::created(user, location))
}
