//! The signed-in user's own account: changing their password.

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use mimorya_core::{DomainError, ErrorCode};
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

use crate::app::AppState;
use crate::auth::credentials::MIN_PASSWORD_CHARS;
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::Auth;
use crate::services::account;

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
    let access = state.access.context(identity).await?;
    // Guessing the current password counts against the account like a login.
    let account_key = access.profile.email.to_lowercase();
    if !state.limits.failed_logins.allow(&account_key) {
        return Err(ApiError::RateLimited);
    }
    let changed = account::change_password(
        &state.db,
        &access,
        identity.session_id,
        body.current_password,
        body.new_password,
    )
    .await;
    // Only a wrong current password counts against the account; a change
    // refused because of a reset or a revocation made meanwhile does not.
    let wrong_password = matches!(&changed, Err(error)
        if error.downcast_ref::<DomainError>().is_some_and(|e| e.code == ErrorCode::Forbidden));
    if !wrong_password {
        state.limits.failed_logins.release(&account_key);
    }
    changed?;
    Ok(Json(json!({ "ok": true })))
}
