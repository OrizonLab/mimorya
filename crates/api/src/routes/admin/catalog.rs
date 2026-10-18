//! What the platform is configured with: roles and the permission catalog,
//! connector instances and types, skills.

use axum::Json;
use axum::extract::State;
use mimorya_connectors::connector_types;
use mimorya_core::authz::{PLATFORM_PERMISSIONS, PermissionInfo};
use mimorya_db::repositories::connectors::list_connector_instances;
use mimorya_db::repositories::skills::list_skills;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::http::{ApiResult, iso};
use crate::plugins::Auth;

pub async fn roles(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require("role:manage")?;
    let roles: Vec<Value> = access
        .roles
        .iter()
        .map(|r| {
            json!({
                "id": r.definition.id,
                "key": r.definition.key,
                "name": r.definition.name,
                "description": r.description,
                "isSystem": r.is_system,
                "permissions": r.definition.permissions,
            })
        })
        .collect();
    let catalog: Vec<PermissionInfo> = PLATFORM_PERMISSIONS
        .iter()
        .copied()
        .chain(
            connector_types()
                .iter()
                .flat_map(|c| c.info().permissions.iter().copied()),
        )
        .collect();
    Ok(Json(json!({ "roles": roles, "catalog": catalog })))
}

pub async fn connectors(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require("connector:manage")?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let instances = list_connector_instances(&mut tx, false).await?;
    tx.commit().await?;
    let instances: Vec<Value> = instances
        .iter()
        .map(|i| {
            json!({
                "id": i.id,
                "type": i.connector_type,
                "name": i.name,
                "enabled": i.enabled,
                "status": i.status,
                "toolPolicies": i.tool_policies,
                "hasSecret": i.secret_id.is_some(),
                "updatedAt": iso(&i.updated_at),
            })
        })
        .collect();
    let types: Vec<Value> = connector_types()
        .iter()
        .map(|c| {
            let info = c.info();
            json!({
                "type": info.connector_type,
                "name": info.name,
                "description": info.description,
                "demo": info.demo,
            })
        })
        .collect();
    Ok(Json(json!({ "instances": instances, "types": types })))
}

/// Every member reads skills through the agent; this screen is for their managers.
pub async fn skills(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require("skill:manage")?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let skills = list_skills(&mut tx).await?;
    tx.commit().await?;
    let skills: Vec<Value> = skills
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "description": s.description,
                "status": s.status,
                "activeVersion": s.active_version,
                "updatedAt": iso(&s.updated_at),
            })
        })
        .collect();
    Ok(Json(json!({ "skills": skills })))
}
