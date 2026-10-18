//! The signed-in user's own account: changing their password (e.g. after an
//! administrator gave them a temporary one).

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::users::{
    get_password_hash, get_user, revoke_user_sessions, set_password_hash,
};
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

use crate::app::AppState;
use crate::auth::credentials::{MIN_PASSWORD_CHARS, hash_password_async, verify_password_async};
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::Auth;

pub fn routes() -> Router<AppState> {
    Router::new().route("/account/password", post(change_password))
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
struct PasswordBody {
    #[validate(length(min = 1, max = 512))]
    current_password: String,
    #[validate(length(min = "MIN_PASSWORD_CHARS", max = 512))]
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<PasswordBody>,
) -> ApiResult<Json<Value>> {
    if body.new_password == body.current_password {
        return Err(DomainError::validation("Le nouveau mot de passe doit être différent").into());
    }
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let Some(user) = get_user(&mut tx, identity.user_id).await? else {
        return Err(DomainError::unauthenticated("Authentification requise").into());
    };
    let current_hash = get_password_hash(&mut tx, identity.user_id).await?;
    tx.commit().await?;

    // Guessing the current password counts against the account like a login.
    let account = user.email.to_lowercase();
    if !state.limits.failed_logins.allow(&account) {
        return Err(ApiError::RateLimited);
    }
    if !verify_password_async(current_hash, body.current_password).await {
        return Err(DomainError::forbidden("Mot de passe actuel incorrect").into());
    }
    state.limits.failed_logins.release(&account);

    let hash = hash_password_async(body.new_password).await?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    set_password_hash(&mut tx, identity.user_id, &hash).await?;
    // Other sessions (another device, a stolen cookie) end; this one stays.
    revoke_user_sessions(&mut tx, identity.user_id, Some(identity.session_id)).await?;
    append_audit(
        &mut tx,
        AuditEntry::new(
            identity.tenant_id,
            "user",
            "user.password_changed",
            "success",
        )
        .by_user(identity.user_id)
        .resource("user", identity.user_id),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
