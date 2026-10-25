//! The evaluation cases of the company's skills (`skill:manage`,
//! company-wide, ADR-103): one written for a skill, then changed or deleted
//! through its own identifier. The skill's detail lists them (`skills.rs`).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::app::AppState;
use crate::contract::admin::SkillEvalCaseInput;
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{EvalCaseInput, change_eval_case, create_eval_case, remove_eval_case};

fn input(body: SkillEvalCaseInput) -> EvalCaseInput {
    EvalCaseInput {
        name: body.name,
        request: body.request,
        criteria: body.criteria,
    }
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(skill_id): PathId,
    Body(body): Body<SkillEvalCaseInput>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let id = create_eval_case(&state.db, &access, skill_id, input(body)).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<SkillEvalCaseInput>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    change_eval_case(&state.db, &access, id, input(body)).await?;
    Ok(Json(Done::OK))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    remove_eval_case(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}
