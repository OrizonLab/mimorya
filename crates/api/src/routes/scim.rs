//! Where the company's identity provider provisions its accounts through
//! SCIM 2.0 (ADR-183), before any session: its token, sent as
//! `Authorization: Bearer <token>`, is its only proof; a session cookie
//! proves nothing here. Every route of the prefix is the provider's alone:
//! a change it sends goes without the request header the browser's carry
//! (`plugins::csrf`). Answers and refusals are SCIM's
//! (`application/scim+json`).

use std::collections::HashMap;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::get;

use crate::app::AppState;
use crate::contract::{ScimServiceProviderConfig, ScimUser, ScimUserList, scim_url};
use crate::http::bearer;
use crate::http::scim::{ScimBody, ScimJson, ScimResult};
use crate::services::scim::{
    ScimPage, authenticate, create_scim_user, list_scim_users, read_scim_user,
};

/// What every SCIM route's path starts with.
pub const PREFIX: &str = "/scim/v2/";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/scim/v2/ServiceProviderConfig", get(config))
        .route("/scim/v2/Users", get(list).post(create))
        .route("/scim/v2/Users/{id}", get(read))
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

async fn read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ScimResult<ScimJson<ScimUser>> {
    let user = read_scim_user(&state.db, bearer(&headers), id.parse().ok(), &base(&state)).await?;
    Ok(ScimJson::ok(user))
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
