//! Access policies (`policy:manage`, company-wide): reading all of them, and
//! creating, changing or deleting deny policies (ADR-022). The listing also
//! gives what a policy may restrict and the attributes a condition can read.

use std::collections::BTreeSet;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::authz::Enforcement;
use mimorya_core::policy_rules::{KNOWN_ATTRIBUTES, is_restrictable};
use mimorya_db::repositories::access_policies::list_policies;
use mimorya_db::repositories::users::list_attribute_keys;

use crate::app::AppState;
use crate::contract::admin::{
    ConditionAttribute, NewPolicy, Policies, PolicyUpdate, PolicyView, RestrictablePermission,
};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{
    PolicyChanges, PolicyInput, create_policy, delete_policy, update_policy,
};
use crate::services::permission_catalog::permission_catalog;

const PERMISSION: &str = "policy:manage";

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<Policies>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let policies = list_policies(&mut tx).await?;
    let attribute_keys = list_attribute_keys(&mut tx).await?;
    tx.commit().await?;
    let policies = policies
        .iter()
        .map(PolicyView::from_row)
        .collect::<anyhow::Result<_>>()?;

    let catalog = permission_catalog();
    // What a policy may restrict, and whether a condition on the resource
    // applies to it (checked resource by resource).
    let permissions = catalog
        .iter()
        .filter(|p| is_restrictable(p.permission, &catalog))
        .map(|p| RestrictablePermission {
            permission: p.permission.into(),
            label: p.label.into(),
            per_resource: p.enforcement == Enforcement::PerResource,
        })
        .collect();
    let domains: BTreeSet<&str> = catalog
        .iter()
        .filter_map(|p| p.permission.split(':').next())
        .filter(|domain| is_restrictable(&format!("{domain}:*"), &catalog))
        .collect();
    // The collaborators' own attributes, as the company named them.
    let attributes = KNOWN_ATTRIBUTES
        .iter()
        .map(|(path, label)| ConditionAttribute {
            path: (*path).into(),
            label: (*label).into(),
        })
        .chain(attribute_keys.iter().map(|key| ConditionAttribute {
            path: format!("subject.attributes.{key}"),
            label: format!("Attribut du collaborateur : {key}"),
        }))
        .collect();
    Ok(Json(Policies {
        policies,
        permissions,
        domains: domains.into_iter().map(String::from).collect(),
        attributes,
    }))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewPolicy>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let catalog = permission_catalog();
    let input = PolicyInput {
        description: body.description,
        permissions: body.permissions,
        condition: body.condition,
        enabled: body.enabled.unwrap_or(true),
    };
    let id = create_policy(&state.db, &state.access, &access, &catalog, input).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<PolicyUpdate>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let catalog = permission_catalog();
    let changes = PolicyChanges {
        description: body.description,
        permissions: body.permissions,
        condition: body.condition,
        enabled: body.enabled,
    };
    update_policy(&state.db, &state.access, &access, &catalog, id, changes).await?;
    Ok(Json(Done::OK))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    delete_policy(&state.db, &state.access, &access, id).await?;
    Ok(Json(Done::OK))
}
