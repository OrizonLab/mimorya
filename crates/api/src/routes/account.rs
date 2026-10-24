//! The signed-in user's own account: changing their password, choosing the
//! e-mails they receive (ADR-094).

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_db::repositories::notification_settings::NotificationSettingsRow;

use crate::app::AppState;
use crate::contract::{Done, NotificationSettings, PasswordUpdate};
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::{Auth, AuthPendingPasswordChange};
use crate::services::account::{self, PasswordChange};
use crate::services::notification_settings::{
    own_notification_settings, set_own_notification_settings,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/account/password", post(change_password))
        .route(
            "/account/notifications",
            get(notifications).put(choose_notifications),
        )
}

async fn change_password(
    State(state): State<AppState>,
    // The route that lets an account replace its temporary password.
    AuthPendingPasswordChange(identity): AuthPendingPasswordChange,
    Body(body): Body<PasswordUpdate>,
) -> ApiResult<Json<Done>> {
    if body.new_password == body.current_password {
        return Err(DomainError::validation("Le nouveau mot de passe doit être différent").into());
    }
    let access = state.access.context(identity).await?;
    // Guessing the current password counts against the account like a login.
    let account_key = access.profile.email.to_lowercase();
    if !state.limits.failed_logins.allow(&account_key).await? {
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
        state.limits.failed_logins.release(&account_key).await;
    }
    match outcome? {
        PasswordChange::Changed => Ok(Json(Done::OK)),
        PasswordChange::WrongCurrentPassword => {
            Err(DomainError::forbidden("Mot de passe actuel incorrect").into())
        }
    }
}

async fn notifications(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<NotificationSettings>> {
    let access = state.access.context(identity).await?;
    let chosen = own_notification_settings(&state.db, &access).await?;
    Ok(Json(NotificationSettings {
        approvals: chosen.approvals,
        own_runs: chosen.own_runs,
        mission_ends: chosen.mission_ends,
    }))
}

async fn choose_notifications(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NotificationSettings>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let chosen = NotificationSettingsRow {
        approvals: body.approvals,
        own_runs: body.own_runs,
        mission_ends: body.mission_ends,
    };
    set_own_notification_settings(&state.db, &access, chosen).await?;
    Ok(Json(Done::OK))
}
