//! The signed-in person's inbound hooks (ADR-109): what their agent does
//! when another system posts to one, listed, created, changed, paused, given
//! a new token and deleted. Their own resource: anyone else's answers as
//! absent. A token is shown once, when made.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};

use crate::app::AppState;
use crate::contract::{
    Done, InboundHook, InboundHookList, InboundHookToken, InboundHookUpdate, NewInboundHook,
    hook_path,
};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::inbound_hooks::{
    HookChanges, HookInput, MAX_BODY_BYTES, MAX_HOOKS, MAX_PER_HOUR, change_hook, create_hook,
    delete_hook, inbound_hooks, replace_token,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/inbound-hooks", get(list).post(create))
        .route("/inbound-hooks/{id}", patch(update).delete(remove))
        .route("/inbound-hooks/{id}/token", post(new_token))
}

async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<InboundHookList>> {
    let access = state.access.context(identity).await?;
    let rows = inbound_hooks(&state.db, &access).await?;
    let hooks = rows
        .iter()
        .map(InboundHook::from_row)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Json(InboundHookList {
        hooks,
        max_hooks: u32::try_from(MAX_HOOKS).unwrap_or(u32::MAX),
        max_per_hour: u32::try_from(MAX_PER_HOUR).unwrap_or(u32::MAX),
        max_body_bytes: u32::try_from(MAX_BODY_BYTES).unwrap_or(u32::MAX),
    }))
}

async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewInboundHook>,
) -> ApiResult<(StatusCode, Json<InboundHookToken>)> {
    let access = state.access.context(identity).await?;
    let input = HookInput {
        title: body.title,
        instruction: body.instruction,
        enabled: body.enabled.unwrap_or(true),
    };
    let (id, token) = create_hook(&state.db, &access, input).await?;
    let made = InboundHookToken {
        id,
        path: hook_path(id),
        token,
    };
    Ok((StatusCode::CREATED, Json(made)))
}

async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<InboundHookUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let changes = HookChanges {
        title: body.title,
        instruction: body.instruction,
        enabled: body.enabled,
    };
    change_hook(&state.db, &access, id, changes).await?;
    Ok(Json(Done::OK))
}

async fn new_token(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<InboundHookToken>> {
    let access = state.access.context(identity).await?;
    let token = replace_token(&state.db, &access, id).await?;
    Ok(Json(InboundHookToken {
        id,
        path: hook_path(id),
        token,
    }))
}

async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    delete_hook(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}
