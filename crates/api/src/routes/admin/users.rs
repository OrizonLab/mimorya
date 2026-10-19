//! The user directory (`user:read`) and account management (`user:manage`,
//! company-wide). Emails, logins and rights are only shown to administrators.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use mimorya_core::authz::Sensitivity;
use mimorya_db::repositories::role_assignments::{RoleAssignmentRow, list_role_assignments};
use mimorya_db::repositories::users::list_users;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;
use validator::Validate;

use crate::app::AppState;
use crate::http::{
    ApiResult, Body, PathId, iso_opt, nullable, trimmed, trimmed_nullable, trimmed_opt,
};
use crate::plugins::Auth;
use crate::services::access::{AccessContext, describe_scope};
use crate::services::admin::{NewUserInput, UserChanges, create_user, reset_password, update_user};

pub async fn list(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require("user:read")?;
    let manage_users = access.require_tenant_wide("user:manage").is_ok();
    let manage_roles = access.require_tenant_wide("role:manage").is_ok();
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let users = list_users(&mut tx).await?;
    let assignments = if manage_roles {
        list_role_assignments(&mut tx).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;

    let tree = access.org_tree();
    let users: Vec<Value> = users
        .iter()
        .map(|u| {
            let mut view = json!({
                "id": u.id,
                "displayName": u.display_name,
                "jobTitle": u.job_title,
                "status": u.status,
                "orgUnit": u.primary_unit_id.map(|unit| tree.path_label(unit)),
            });
            if manage_users || manage_roles {
                view["email"] = json!(u.email);
                view["lastLoginAt"] = iso_opt(&u.last_login_at);
                view["orgUnitId"] = json!(u.primary_unit_id);
                view["clearance"] = json!(u.clearance);
            }
            // Only to those who set them (ADR-023).
            if manage_users {
                view["attributes"] = u.attributes.clone();
            }
            if manage_roles {
                let own: Vec<&RoleAssignmentRow> =
                    assignments.iter().filter(|a| a.user_id == u.id).collect();
                view["grants"] = json!(
                    own.iter()
                        .map(|a| grant_label(&access, a))
                        .collect::<Vec<_>>()
                );
                view["assignments"] = json!(
                    own.iter()
                        .map(|a| assignment_view(&access, a))
                        .collect::<Vec<_>>()
                );
            }
            view
        })
        .collect();
    Ok(Json(json!({
        "users": users,
        "canManage": { "users": manage_users, "roles": manage_roles },
    })))
}

fn grant_label(access: &AccessContext, a: &RoleAssignmentRow) -> String {
    format!(
        "{} — {}",
        a.role_name,
        describe_scope(&a.scope(), access.org_tree())
    )
}

fn assignment_view(access: &AccessContext, a: &RoleAssignmentRow) -> Value {
    json!({
        "id": a.id,
        "roleId": a.role_id,
        "roleName": a.role_name,
        "scope": a.scope(),
        "label": grant_label(access, a),
        "expiresAt": iso_opt(&a.expires_at),
    })
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserBody {
    #[serde(deserialize_with = "trimmed")]
    #[validate(email, length(max = 254))]
    email: String,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 120))]
    display_name: String,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 120))]
    job_title: Option<String>,
    #[serde(default)]
    org_unit_id: Option<Uuid>,
    #[serde(default)]
    clearance: Sensitivity,
}

/// Responses carrying a temporary password are never stored by a cache.
type Secret = ([(axum::http::HeaderName, &'static str); 1], Json<Value>);

fn secret(body: Value) -> Secret {
    ([(CACHE_CONTROL, "no-store")], Json(body))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<CreateUserBody>,
) -> ApiResult<(StatusCode, Secret)> {
    let access = state.access.context(identity).await?;
    let input = NewUserInput {
        email: body.email,
        display_name: body.display_name,
        job_title: body.job_title.filter(|t| !t.is_empty()),
        org_unit_id: body.org_unit_id,
        clearance: body.clearance,
    };
    let (id, password) = create_user(&state.db, &access, input).await?;
    Ok((
        StatusCode::CREATED,
        secret(json!({ "id": id, "temporaryPassword": password })),
    ))
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum AccountStatus {
    Active,
    Disabled,
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserBody {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 2, max = 120))]
    display_name: Option<String>,
    #[serde(default, deserialize_with = "trimmed_nullable")]
    job_title: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    org_unit_id: Option<Option<Uuid>>,
    #[serde(default)]
    clearance: Option<Sensitivity>,
    #[serde(default)]
    status: Option<AccountStatus>,
    /// Replaces every attribute; `{}` removes them all.
    #[serde(default)]
    attributes: Option<Map<String, Value>>,
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<UpdateUserBody>,
) -> ApiResult<Json<Value>> {
    if body
        .job_title
        .as_ref()
        .and_then(Option::as_ref)
        .is_some_and(|t| t.chars().count() > 120)
    {
        return Err(crate::http::ApiError::invalid(vec![
            "jobTitle : longueur maximale 120".into(),
        ]));
    }
    let access = state.access.context(identity).await?;
    let changes = UserChanges {
        display_name: body.display_name,
        job_title: body.job_title,
        org_unit_id: body.org_unit_id,
        clearance: body.clearance,
        active: body.status.map(|s| s == AccountStatus::Active),
        attributes: body.attributes,
    };
    update_user(&state.db, &access, id, changes).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn reset(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Secret> {
    let access = state.access.context(identity).await?;
    let password = reset_password(&state.db, &access, id).await?;
    Ok(secret(json!({ "temporaryPassword": password })))
}
