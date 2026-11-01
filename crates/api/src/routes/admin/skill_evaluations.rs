//! The evaluations of the company's skills (`skill:manage`, company-wide,
//! ADR-104): one asked for, the skill's listed with what the company's
//! evaluations cost over thirty days (ADR-154), and one read case by case,
//! its answers shown to whom it ran for only.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::DomainError;
use mimorya_db::repositories::skills::get_skill;

use crate::app::AppState;
use crate::contract::Created;
use crate::contract::admin::{
    EvaluationRequest, SkillEvaluationDetail, SkillEvaluationView, SkillEvaluations,
};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::launch_evaluation;
use crate::services::skill_evaluations::{evaluation, evaluations_of, shown_to, spending};

const PERMISSION: &str = "skill:manage";

pub async fn launch(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(skill_id): PathId,
    Body(body): Body<EvaluationRequest>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let id = launch_evaluation(&state, &access, skill_id, body.version).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(skill_id): PathId,
) -> ApiResult<Json<SkillEvaluations>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    if get_skill(&mut tx, skill_id).await?.is_none() {
        return Err(DomainError::not_found("Compétence introuvable").into());
    }
    let evaluations = evaluations_of(&mut tx, skill_id).await?;
    let spending = spending(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(SkillEvaluations {
        evaluations: evaluations
            .iter()
            .map(SkillEvaluationView::from_read)
            .collect::<anyhow::Result<_>>()?,
        spent_micros: spending.spent_micros,
        max_cost_micros: spending.limit_micros,
    }))
}

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<SkillEvaluationDetail>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let read = evaluation(&mut tx, id)
        .await?
        .ok_or_else(|| DomainError::not_found("Évaluation introuvable"))?;
    tx.commit().await?;
    let shown = shown_to(&read, access.subject.user_id);
    Ok(Json(SkillEvaluationDetail::new(&read, shown)?))
}
