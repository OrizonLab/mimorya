//! The user directory (`user:read`) and account management (`user:manage`,
//! company-wide): a new account gets an invitation link (ADR-027), e-mailed
//! or shown once; a password is reset by an e-mailed link (ADR-029) or a
//! temporary password shown once. Emails, logins and rights are only shown
//! to administrators.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use mimorya_core::accounts::{DISPLAY_NAME_MAX_CHARS, DISPLAY_NAME_MIN_CHARS, EMAIL_MAX_LEN};
use mimorya_core::authz::Sensitivity;
use mimorya_db::repositories::invitations::pending_invitations;
use mimorya_db::repositories::role_assignments::{RoleAssignmentRow, list_role_assignments};
use mimorya_db::repositories::users::list_users;
use serde::Deserialize;
use serde_json::{Map, Value};
use uuid::Uuid;
use validator::Validate;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{
    Assignment, Directory, DirectoryRights, DirectoryUser, InvitationIssued, ResetLinkSent,
    TemporaryPassword, UserCreated,
};
use crate::contract::stored_text;
use crate::http::{ApiResult, Body, PathId, iso, nullable, trimmed, trimmed_nullable, trimmed_opt};
use crate::plugins::Auth;
use crate::services::access::{AccessContext, describe_scope};
use crate::services::admin::{
    LinkSender, NewUserInput, UserChanges, create_user, issue_invitation, reset_password,
    send_reset_link, update_user,
};

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<Directory>> {
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
    let invitations = if manage_users {
        pending_invitations(&mut tx).await?
    } else {
        HashMap::new()
    };
    tx.commit().await?;

    let tree = access.org_tree();
    let users = users
        .iter()
        .map(|u| {
            let admin = manage_users || manage_roles;
            let own: Vec<&RoleAssignmentRow> =
                assignments.iter().filter(|a| a.user_id == u.id).collect();
            Ok(DirectoryUser {
                id: u.id,
                display_name: u.display_name.clone(),
                job_title: u.job_title.clone(),
                status: stored_text("account status", &u.status)?,
                org_unit: u.primary_unit_id.map(|unit| tree.path_label(unit)),
                email: admin.then(|| u.email.clone()),
                last_login_at: admin.then(|| u.last_login_at.as_ref().map(iso)),
                org_unit_id: admin.then_some(u.primary_unit_id),
                clearance: admin
                    .then(|| stored_text("account clearance", &u.clearance))
                    .transpose()?,
                // Only to those who set them (ADR-023).
                attributes: manage_users.then(|| u.attributes.clone()),
                invitation_expires_at: manage_users.then(|| invitations.get(&u.id).map(iso)),
                grants: manage_roles.then(|| own.iter().map(|a| grant_label(&access, a)).collect()),
                assignments: manage_roles
                    .then(|| own.iter().map(|a| assignment_view(&access, a)).collect()),
            })
        })
        .collect::<anyhow::Result<_>>()?;
    Ok(Json(Directory {
        users,
        can_manage: DirectoryRights {
            users: manage_users,
            roles: manage_roles,
        },
        email_enabled: state.mailer.is_some(),
    }))
}

fn grant_label(access: &AccessContext, a: &RoleAssignmentRow) -> String {
    format!(
        "{} — {}",
        a.role_name,
        describe_scope(&a.scope(), access.org_tree())
    )
}

fn assignment_view(access: &AccessContext, a: &RoleAssignmentRow) -> Assignment {
    Assignment {
        id: a.id,
        role_id: a.role_id,
        role_name: a.role_name.clone(),
        scope: (&a.scope()).into(),
        label: grant_label(access, a),
        expires_at: a.expires_at.as_ref().map(iso),
    }
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserBody {
    #[serde(deserialize_with = "trimmed")]
    #[validate(email, length(max = EMAIL_MAX_LEN))]
    email: String,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = DISPLAY_NAME_MIN_CHARS, max = DISPLAY_NAME_MAX_CHARS))]
    display_name: String,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 120))]
    job_title: Option<String>,
    #[serde(default)]
    org_unit_id: Option<Uuid>,
    #[serde(default)]
    clearance: Sensitivity,
}

/// Responses carrying a temporary password or an invitation link are never
/// stored by a cache.
type Secret<T> = ([(axum::http::HeaderName, &'static str); 1], Json<T>);

fn secret<T>(body: T) -> Secret<T> {
    ([(CACHE_CONTROL, "no-store")], Json(body))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<CreateUserBody>,
) -> ApiResult<(StatusCode, Secret<UserCreated>)> {
    let access = state.access.context(identity).await?;
    let input = NewUserInput {
        email: body.email,
        display_name: body.display_name,
        job_title: body.job_title.filter(|t| !t.is_empty()),
        org_unit_id: body.org_unit_id,
        clearance: body.clearance,
    };
    let id = create_user(&state.db, &access, input).await?;
    let invitation = issue_invitation(&link_sender(&state), &access, id).await?;
    Ok((
        StatusCode::CREATED,
        secret(UserCreated {
            id,
            invitation: (&invitation).into(),
        }),
    ))
}

/// A new link for an account that never chose its password; the former
/// one stops working.
pub async fn invite(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Secret<InvitationIssued>> {
    let access = state.access.context(identity).await?;
    let invitation = issue_invitation(&link_sender(&state), &access, id).await?;
    Ok(secret(InvitationIssued {
        invitation: (&invitation).into(),
    }))
}

fn link_sender(state: &AppState) -> LinkSender<'_> {
    LinkSender {
        db: &state.db,
        mailer: state.mailer.as_deref(),
        web_origin: &state.config.web_origin,
        product: &state.config.assistant_name,
    }
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum StatusChange {
    Active,
    Disabled,
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserBody {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = DISPLAY_NAME_MIN_CHARS, max = DISPLAY_NAME_MAX_CHARS))]
    display_name: Option<String>,
    #[serde(default, deserialize_with = "trimmed_nullable")]
    job_title: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    org_unit_id: Option<Option<Uuid>>,
    #[serde(default)]
    clearance: Option<Sensitivity>,
    #[serde(default)]
    status: Option<StatusChange>,
    /// Replaces every attribute; `{}` removes them all.
    #[serde(default)]
    attributes: Option<Map<String, Value>>,
}

pub async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<UpdateUserBody>,
) -> ApiResult<Json<Done>> {
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
        active: body.status.map(|s| s == StatusChange::Active),
        attributes: body.attributes,
    };
    update_user(&state.db, &access, id, changes).await?;
    Ok(Json(Done::OK))
}

pub async fn reset(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Secret<TemporaryPassword>> {
    let access = state.access.context(identity).await?;
    let password = reset_password(&state.db, &access, id).await?;
    Ok(secret(TemporaryPassword {
        temporary_password: password,
    }))
}

/// A reset link e-mailed to the account (ADR-029); the administrator never
/// sees it.
pub async fn send_link(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<ResetLinkSent>> {
    let access = state.access.context(identity).await?;
    let expires_at = send_reset_link(&link_sender(&state), &access, id).await?;
    Ok(Json(ResetLinkSent {
        expires_at: iso(&expires_at),
    }))
}
