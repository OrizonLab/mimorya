//! The company's skills (`skill:manage`, company-wide, ADR-052): the list, a
//! skill with its versions, and their changes. Every member reads the active
//! skills through the agent (`skill:read`); these screens are their managers'.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::DomainError;
use mimorya_db::repositories::skills::{get_skill, list_skill_versions, list_skills};

use crate::app::AppState;
use crate::contract::admin::{
    NewSkill, NewSkillVersion, SkillActivation, SkillDetail, SkillList, SkillUpdate,
    SkillVersionCreated, SkillVersionView, SkillView,
};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{
    SkillInput, activate_skill, add_skill_version, create_skill, delete_skill, retire_skill,
    update_skill,
};

const PERMISSION: &str = "skill:manage";

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<SkillList>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let skills = list_skills(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(SkillList {
        skills: skills
            .iter()
            .map(SkillView::from_row)
            .collect::<anyhow::Result<_>>()?,
    }))
}

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<SkillDetail>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let skill = get_skill(&mut tx, id)
        .await?
        .ok_or_else(|| DomainError::not_found("Compétence introuvable"))?;
    let versions = list_skill_versions(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(SkillDetail {
        skill: SkillView::from_row(&skill)?,
        versions: versions
            .iter()
            .map(SkillVersionView::from_row)
            .collect::<anyhow::Result<_>>()?,
    }))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewSkill>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let input = SkillInput {
        name: body.name,
        description: body.description,
        content: body.content,
        changelog: body.changelog.unwrap_or_default(),
    };
    let id = create_skill(&state.db, &access, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<SkillUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    update_skill(&state.db, &access, id, body.description).await?;
    Ok(Json(Done::OK))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    delete_skill(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}

pub async fn add_version(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<NewSkillVersion>,
) -> ApiResult<(StatusCode, Json<SkillVersionCreated>)> {
    let access = state.access.context(identity).await?;
    let version = add_skill_version(
        &state.db,
        &access,
        id,
        body.content,
        body.changelog.unwrap_or_default(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(SkillVersionCreated { version })))
}

pub async fn activate(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<SkillActivation>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    activate_skill(&state.db, &access, id, body.version).await?;
    Ok(Json(Done::OK))
}

pub async fn retire(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    retire_skill(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}
