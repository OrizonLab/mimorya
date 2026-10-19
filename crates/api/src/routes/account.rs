//! The signed-in user's own account: changing their password.

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use mimorya_core::DomainError;
use serde::Deserialize;
use validator::Validate;

use crate::app::AppState;
use crate::auth::credentials::MIN_PASSWORD_CHARS;
use crate::contract::Done;
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::AuthPendingPasswordChange;
use crate::services::account::{self, PasswordChange};

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
    // The route that lets an account replace its temporary password.
    AuthPendingPasswordChange(identity): AuthPendingPasswordChange,
    Body(body): Body<PasswordBody>,
) -> ApiResult<Json<Done>> {
    if body.new_password == body.current_password {
        return Err(DomainError::validation("Le nouveau mot de passe doit être différent").into());
    }
    let access = state.access.context(identity).await?;
    // Guessing the current password counts against the account like a login.
    let account_key = access.profile.email.to_lowercase();
    if !state.limits.failed_logins.allow(&account_key) {
        return Err(ApiError::RateLimited);
    }
    let outcome = account::change_password(
        &state.db,
        &access,
        identity.session_id,
        body.current_password,
        body.new_password,
    )
    .await;
    // Only a wrong current password counts against the account; a change
    // refused because of a reset or a revocation made meanwhile does not.
    if !matches!(outcome, Ok(PasswordChange::WrongCurrentPassword)) {
        state.limits.failed_logins.release(&account_key);
    }
    match outcome? {
        PasswordChange::Changed => Ok(Json(Done::OK)),
        PasswordChange::WrongCurrentPassword => {
            Err(DomainError::forbidden("Mot de passe actuel incorrect").into())
        }
    }
}
