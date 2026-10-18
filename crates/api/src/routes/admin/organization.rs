//! The organisation tree: reading it (`org:read`) and changing it
//! (`org:manage`, company-wide).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::DomainError;
use mimorya_core::authz::OrgUnitType;
use mimorya_db::repositories::users::list_users;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
use validator::Validate;

use crate::app::AppState;
use crate::http::{ApiResult, Body, PathId, nullable, trimmed, trimmed_opt};
use crate::plugins::Auth;
use crate::services::admin::{NewUnitInput, UnitChanges, create_unit, delete_unit, update_unit};

pub async fn organization(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require("org:read")?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let users = list_users(&mut tx).await?;
    tx.commit().await?;
    let units: Vec<Value> = access
        .org_tree()
        .all()
        .iter()
        .map(|u| {
            let members: Vec<_> = users
                .iter()
                .filter(|m| m.primary_unit_id == Some(u.id))
                .map(|m| m.id)
                .collect();
            json!({
                "id": u.id,
                "parentId": u.parent_id,
                "name": u.name,
                "type": u.unit_type.as_str(),
                "leadUserId": u.lead_user_id,
                "members": members,
            })
        })
        .collect();
    let users: Vec<Value> = users
        .iter()
        .map(|u| json!({ "id": u.id, "displayName": u.display_name, "jobTitle": u.job_title }))
        .collect();
    let can_manage = access.require_tenant_wide("org:manage").is_ok();
    Ok(Json(
        json!({ "units": units, "users": users, "canManage": can_manage }),
    ))
}

fn unit_type(value: &str) -> Result<OrgUnitType, DomainError> {
    OrgUnitType::parse(value).ok_or_else(|| DomainError::validation("Type d’unité inconnu"))
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateUnitBody {
    parent_id: Uuid,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 120))]
    name: String,
    #[serde(rename = "type")]
    unit_type: String,
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<CreateUnitBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let access = state.access.context(identity).await?;
    let input = NewUnitInput {
        parent_id: body.parent_id,
        name: body.name,
        unit_type: unit_type(&body.unit_type)?,
    };
    let id = create_unit(&state.db, &state.access, &access, input).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUnitBody {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 1, max = 120))]
    name: Option<String>,
    #[serde(default, rename = "type")]
    unit_type: Option<String>,
    #[serde(default)]
    parent_id: Option<Uuid>,
    #[serde(default, deserialize_with = "nullable")]
    lead_user_id: Option<Option<Uuid>>,
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<UpdateUnitBody>,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    let changes = UnitChanges {
        name: body.name,
        unit_type: body.unit_type.as_deref().map(unit_type).transpose()?,
        parent_id: body.parent_id,
        lead_user_id: body.lead_user_id,
    };
    update_unit(&state.db, &state.access, &access, id, changes).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    delete_unit(&state.db, &state.access, &access, id).await?;
    Ok(Json(json!({ "ok": true })))
}
