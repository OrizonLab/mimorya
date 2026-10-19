//! Roles and the permission catalog (`role:manage`, company-wide): reading
//! them, and creating, changing or deleting custom roles.

use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::authz::PermissionInfo;
use mimorya_db::repositories::roles::list_roles;
use uuid::Uuid;

use crate::app::AppState;
use crate::contract::admin::{NewRole, RoleUpdate, RoleView, Roles};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{RoleChanges, RoleInput, create_role, delete_role, update_role};
use crate::services::permission_catalog::permission_catalog;

pub async fn list(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Roles>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide("role:manage")?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let roles = list_roles(&mut tx).await?;
    tx.commit().await?;
    let held: HashSet<Uuid> = access
        .subject
        .assignments
        .iter()
        .map(|a| a.role_id)
        .collect();
    Ok(Json(Roles {
        roles: roles
            .iter()
            .map(|r| RoleView::new(r, held.contains(&r.id)))
            .collect(),
        catalog: permission_catalog().iter().map(Into::into).collect(),
    }))
}

fn catalog_keys(catalog: &[PermissionInfo]) -> Vec<&'static str> {
    catalog.iter().map(|p| p.permission).collect()
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewRole>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let catalog = permission_catalog();
    let input = RoleInput {
        name: body.name,
        description: body.description.unwrap_or_default(),
        permissions: body.permissions,
    };
    let id = create_role(
        &state.db,
        &state.access,
        &access,
        &catalog_keys(&catalog),
        input,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<RoleUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let catalog = permission_catalog();
    let changes = RoleChanges {
        name: body.name,
        description: body.description,
        permissions: body.permissions,
    };
    update_role(
        &state.db,
        &state.access,
        &access,
        &catalog_keys(&catalog),
        id,
        changes,
    )
    .await?;
    Ok(Json(Done::OK))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    delete_role(&state.db, &state.access, &access, id).await?;
    Ok(Json(Done::OK))
}
