//! Organisation tree and user directory. Emails, last logins and role grants
//! are only shown to those who manage roles.

use axum::Json;
use axum::extract::State;
use mimorya_db::repositories::org::{list_role_assignments, list_users};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::http::{ApiResult, iso_opt};
use crate::plugins::Auth;
use crate::services::access::describe_scope;

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
    Ok(Json(json!({ "units": units, "users": users })))
}

pub async fn users(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require("user:read")?;
    let can_see_roles = access.has("role:manage");
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let users = list_users(&mut tx).await?;
    let assignments = if can_see_roles {
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
            if can_see_roles {
                let grants: Vec<String> = assignments
                    .iter()
                    .filter(|a| a.user_id == u.id)
                    .map(|a| format!("{} — {}", a.role_name, describe_scope(&a.scope(), tree)))
                    .collect();
                view["email"] = json!(u.email);
                view["lastLoginAt"] = iso_opt(&u.last_login_at);
                view["grants"] = json!(grants);
            }
            view
        })
        .collect();
    Ok(Json(json!({ "users": users })))
}
