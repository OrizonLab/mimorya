//! Role assignments of an account (`role:manage`, company-wide).

use crate::app::AppState;
use crate::contract::admin::RoleGrant;
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{grant_role, revoke_role};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

pub async fn grant(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(user_id): PathId,
    Body(body): Body<RoleGrant>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let id = grant_role(
        &state.db,
        &access,
        user_id,
        body.role_id,
        body.scope.into(),
        body.expires_at,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn revoke(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    revoke_role(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}
