//! Sign-in, sign-out, what the sign-in page may offer, and the profile of
//! the signed-in user with the permissions computed server-side.

use std::collections::BTreeSet;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::auth::{find_login, lock_login_state, revoke_session};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
use validator::Validate;

use crate::app::AppState;
use crate::auth::cookies::{cleared_session_cookie, session_cookie};
use crate::auth::credentials::verify_password_async;
use crate::auth::sessions::open_session;
use crate::http::{ApiError, ApiResult, Body, trimmed};
use crate::plugins::{AuthPendingPasswordChange, MaybeAuth, RequestContext};
use crate::services::permission_catalog::permission_catalog;

const INVALID_CREDENTIALS: &str = "Identifiants invalides";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/options", get(options))
        .route("/auth/me", get(me))
}

#[derive(Deserialize, Validate)]
struct LoginBody {
    #[serde(deserialize_with = "trimmed")]
    #[validate(email, length(max = 254))]
    email: String,
    #[validate(length(min = 1, max = 512))]
    password: String,
}

fn login_audit(
    tenant_id: Uuid,
    user_id: Uuid,
    outcome: &'static str,
    context: &RequestContext,
) -> AuditEntry {
    let mut entry = AuditEntry::new(tenant_id, "user", "auth.login", outcome)
        .by_user(user_id)
        .ip(context.ip_string());
    entry.request_id = Some(context.id.to_string());
    entry
}

/// Written off the response path, so that response time does not depend on
/// whether the account exists.
fn record_in_background(state: AppState, entry: AuditEntry) {
    let db = state.db.clone();
    state.background.spawn(async move {
        let tenant_id = entry.tenant_id;
        let written = async {
            let mut tx = db.begin(tenant_id).await?;
            append_audit(&mut tx, entry).await?;
            tx.commit().await
        };
        if let Err(error) = written.await {
            tracing::warn!(error = %error, "Failed to audit a denied login");
        }
    });
}

async fn login(
    State(state): State<AppState>,
    context: RequestContext,
    headers: HeaderMap,
    Body(body): Body<LoginBody>,
) -> ApiResult<impl IntoResponse> {
    if !state.limits.login.allow(&context.client_key()) {
        return Err(ApiError::RateLimited);
    }
    let email = body.email.to_lowercase();
    // The slot is taken before verifying, atomically: concurrent attempts
    // cannot all pass the check. Unknown and existing accounts are treated
    // alike, so nothing reveals which exist.
    if !state.limits.failed_logins.allow(&email) {
        return Err(ApiError::RateLimited);
    }
    let login = find_login(state.db.pool(), &email).await?;
    let valid = verify_password_async(
        login.as_ref().and_then(|l| l.password_hash.clone()),
        body.password,
    )
    .await;
    let login = match login {
        Some(login) if valid && login.status == "active" && login.tenant_status == "active" => {
            // Only failures count against the account.
            state.limits.failed_logins.release(&email);
            login
        }
        other => {
            if let Some(login) = other {
                let reason = if valid {
                    "Compte ou entreprise inactif"
                } else {
                    "Mot de passe invalide"
                };
                let entry =
                    login_audit(login.tenant_id, login.user_id, "denied", &context).reason(reason);
                record_in_background(state.clone(), entry);
            }
            return Err(DomainError::unauthenticated(INVALID_CREDENTIALS).into());
        }
    };

    let mut tx = state.db.begin(login.tenant_id).await?;
    // A reset, a disabling or a password change committed during the
    // verification wins: no session is opened with what it replaced.
    let still_valid = lock_login_state(&mut tx, login.user_id)
        .await?
        .is_some_and(|(hash, status)| hash == login.password_hash && status == "active");
    if !still_valid {
        let entry = login_audit(login.tenant_id, login.user_id, "denied", &context)
            .reason("Compte modifié pendant la connexion");
        record_in_background(state.clone(), entry);
        return Err(DomainError::unauthenticated(INVALID_CREDENTIALS).into());
    }
    let token = open_session(&mut tx, login.user_id, &context, &headers).await?;
    append_audit(
        &mut tx,
        login_audit(login.tenant_id, login.user_id, "success", &context),
    )
    .await?;
    tx.commit().await?;

    let cookie = session_cookie(&token, state.config.secure_cookies);
    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(json!({ "ok": true })),
    ))
}

/// What the sign-in page offers: a reset link can be asked for only when
/// e-mails go out (ADR-028).
async fn options(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "passwordReset": state.mailer.is_some() }))
}

async fn logout(
    State(state): State<AppState>,
    context: RequestContext,
    MaybeAuth(identity): MaybeAuth,
) -> ApiResult<impl IntoResponse> {
    if let Some(identity) = identity {
        let mut tx = state.db.begin(identity.tenant_id).await?;
        revoke_session(&mut tx, identity.session_id).await?;
        let mut entry = AuditEntry::new(identity.tenant_id, "user", "auth.logout", "success")
            .by_user(identity.user_id)
            .ip(context.ip_string());
        entry.request_id = Some(context.id.to_string());
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
    }
    let cookie = cleared_session_cookie(state.config.secure_cookies);
    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(json!({ "ok": true })),
    ))
}

async fn me(
    State(state): State<AppState>,
    AuthPendingPasswordChange(identity): AuthPendingPasswordChange,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    let catalog: BTreeSet<&str> = permission_catalog()
        .into_iter()
        .map(|p| p.permission)
        .collect();
    let permissions: Vec<&str> = catalog.into_iter().filter(|p| access.holds(p)).collect();
    let org_unit = access
        .profile
        .primary_unit_id
        .map(|unit| json!({ "id": unit, "path": access.org_tree().path_label(unit) }));
    Ok(Json(json!({
        "user": {
            "id": access.profile.id,
            "email": access.profile.email,
            "displayName": access.profile.display_name,
            "jobTitle": access.profile.job_title,
            "orgUnit": org_unit,
            "mustChangePassword": identity.must_change_password,
        },
        "tenant": {
            "id": access.tenant.id,
            "name": access.tenant.name,
            "demo": access.is_demo(),
        },
        "grants": access.grants,
        "permissions": permissions,
        "assistant": {
            "name": state.config.assistant_name,
            "available": state.llm.available(access.subject.tenant_id).await,
        },
    })))
}
