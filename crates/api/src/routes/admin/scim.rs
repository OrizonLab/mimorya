//! The tokens with which the company's identity provider provisions its
//! accounts through SCIM (`user:manage`, company-wide, ADR-183): listed
//! without their secret, with where the provider reaches SCIM, made with a
//! name, the secret shown this once, and removed.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{BearerTokenCreated, NewBearerToken, ScimTokens};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::scim::SCIM_TOKENS;

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<ScimTokens>> {
    let access = state.access.context(identity).await?;
    let tokens = SCIM_TOKENS.list(&state.db, &access).await?;
    Ok(Json(ScimTokens::of(&tokens, &state.config.web_origin)))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewBearerToken>,
) -> ApiResult<(StatusCode, Json<BearerTokenCreated>)> {
    let access = state.access.context(identity).await?;
    let (token, secret) = SCIM_TOKENS.create(&state.db, &access, &body.name).await?;
    Ok((
        StatusCode::CREATED,
        Json(BearerTokenCreated::of(&token, secret)),
    ))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    SCIM_TOKENS.delete(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}
