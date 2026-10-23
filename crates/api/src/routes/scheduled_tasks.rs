//! The signed-in person's scheduled tasks (ADR-080): what they ask their
//! agent and when, listed, created, changed, paused and deleted. Their own
//! resource: anyone else's answers as absent.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::Utc;

use crate::app::AppState;
use crate::contract::{
    Created, Done, NewScheduledTask, ScheduledTask, ScheduledTaskList, ScheduledTaskUpdate,
};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::scheduled_tasks::{
    MAX_TASKS, TaskChanges, TaskInput, change_task, create_task, delete_task, scheduled_tasks,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/scheduled-tasks", get(list).post(create))
        .route("/scheduled-tasks/{id}", patch(update).delete(remove))
}

async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<ScheduledTaskList>> {
    let access = state.access.context(identity).await?;
    let rows = scheduled_tasks(&state.db, &access).await?;
    let tasks = rows
        .iter()
        .map(ScheduledTask::from_row)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Json(ScheduledTaskList {
        tasks,
        max_tasks: u32::try_from(MAX_TASKS).unwrap_or(u32::MAX),
    }))
}

async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewScheduledTask>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let input = TaskInput {
        title: body.title,
        instruction: body.instruction,
        weekdays: body.weekdays,
        time: body.time,
        time_zone: body.time_zone,
        enabled: body.enabled.unwrap_or(true),
    };
    let id = create_task(&state.db, &access, input, Utc::now()).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<ScheduledTaskUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let changes = TaskChanges {
        title: body.title,
        instruction: body.instruction,
        weekdays: body.weekdays,
        time: body.time,
        time_zone: body.time_zone,
        enabled: body.enabled,
    };
    change_task(&state.db, &access, id, changes, Utc::now()).await?;
    Ok(Json(Done::OK))
}

async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    delete_task(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}
