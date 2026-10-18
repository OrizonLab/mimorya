//! Role assignments of an account (`role:manage`, company-wide).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use mimorya_core::authz::AssignmentScope;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
use validator::Validate;

use crate::app::AppState;
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{grant_role, revoke_role};

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct GrantBody {
    role_id: Uuid,
    /// `{"type": "tenant"}`, `{"type": "self"}` or
    /// `{"type": "org_unit", "orgUnitId": …, "includeDescendants": …}`.
    scope: AssignmentScope,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
}

pub async fn grant(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(user_id): PathId,
    Body(body): Body<GrantBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let access = state.access.context(identity).await?;
    let id = grant_role(
        &state.db,
        &access,
        user_id,
        body.role_id,
        body.scope,
        body.expires_at,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn revoke(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    revoke_role(&state.db, &access, id).await?;
    Ok(Json(json!({ "ok": true })))
}
