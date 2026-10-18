//! Access policies (`policy:manage`, company-wide): reading all of them, and
//! creating, changing or deleting deny policies (ADR-022). The listing also
//! gives what a policy may restrict and the attributes a condition can read.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::authz::{Condition, PermissionInfo};
use mimorya_core::policy_rules::{KNOWN_ATTRIBUTES, normalize_policy_permissions};
use mimorya_db::repositories::access_policies::list_policies;
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

use crate::app::AppState;
use crate::http::{ApiResult, Body, PathId, iso, trimmed, trimmed_opt};
use crate::plugins::Auth;
use crate::services::admin::{
    PolicyChanges, PolicyInput, create_policy, delete_policy, update_policy,
};
use crate::services::permission_catalog::permission_catalog;

const PERMISSION: &str = "policy:manage";

pub async fn list(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let policies = list_policies(&mut tx).await?;
    tx.commit().await?;
    let policies: Vec<Value> = policies
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "description": p.description,
                "effect": p.effect,
                "permissions": p.permissions,
                "condition": p.condition,
                "enabled": p.enabled,
                "editable": p.effect == "deny",
                "updatedAt": iso(&p.updated_at),
            })
        })
        .collect();

    let catalog = permission_catalog();
    let keys = catalog_keys(&catalog);
    let restrictable =
        |pattern: &str| normalize_policy_permissions(&[pattern.to_string()], &keys).is_ok();
    let permissions: Vec<&PermissionInfo> = catalog
        .iter()
        .filter(|p| restrictable(p.permission))
        .collect();
    let mut domains: Vec<&str> = keys
        .iter()
        .filter_map(|p| p.split(':').next())
        .filter(|domain| restrictable(&format!("{domain}:*")))
        .collect();
    domains.dedup();
    let attributes: Vec<Value> = KNOWN_ATTRIBUTES
        .iter()
        .map(|(path, label)| json!({ "path": path, "label": label }))
        .collect();
    Ok(Json(json!({
        "policies": policies,
        "permissions": permissions,
        "domains": domains,
        "attributes": attributes,
    })))
}

fn catalog_keys(catalog: &[PermissionInfo]) -> Vec<&'static str> {
    catalog.iter().map(|p| p.permission).collect()
}

const fn enabled_by_default() -> bool {
    true
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePolicyBody {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 3, max = 300))]
    description: String,
    #[validate(length(max = 50))]
    permissions: Vec<String>,
    condition: Condition,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<CreatePolicyBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let access = state.access.context(identity).await?;
    let catalog = permission_catalog();
    let input = PolicyInput {
        description: body.description,
        permissions: body.permissions,
        condition: body.condition,
        enabled: body.enabled,
    };
    let id = create_policy(
        &state.db,
        &state.access,
        &access,
        &catalog_keys(&catalog),
        input,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdatePolicyBody {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 3, max = 300))]
    description: Option<String>,
    #[serde(default)]
    #[validate(length(max = 50))]
    permissions: Option<Vec<String>>,
    #[serde(default)]
    condition: Option<Condition>,
    #[serde(default)]
    enabled: Option<bool>,
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<UpdatePolicyBody>,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    let catalog = permission_catalog();
    let changes = PolicyChanges {
        description: body.description,
        permissions: body.permissions,
        condition: body.condition,
        enabled: body.enabled,
    };
    update_policy(
        &state.db,
        &state.access,
        &access,
        &catalog_keys(&catalog),
        id,
        changes,
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    delete_policy(&state.db, &state.access, &access, id).await?;
    Ok(Json(json!({ "ok": true })))
}
