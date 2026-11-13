//! The account the signed-in person links at their company's standard mail
//! (ADR-205): the mail and their account there; linking it, at most so
//! many tries an hour, each signing in to the company's servers; and its
//! removal. Never anyone else's, never the password.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::app::AppState;
use crate::contract::{AccountMail, Done, MailAccountLink};
use crate::http::{ApiError, ApiResult, Body};
use crate::plugins::Auth;
use crate::services::mail_accounts::{
    MailLinking, link_mail_account, mail_account_of, unlink_mail_account,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/account/mail", get(read).put(link).delete(remove))
}

async fn read(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<AccountMail>> {
    let access = state.access.context(identity).await?;
    let mail = mail_account_of(&state.db, &access).await?;
    Ok(Json(AccountMail {
        mail: mail.map(Into::into),
    }))
}

async fn link(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<MailAccountLink>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let store = state.secret_store();
    let linking = MailLinking {
        db: &state.db,
        secrets: &store,
        network: state.connectors.network(),
    };
    let prepared = linking
        .prepare(&access, &body.username, &body.password)
        .await?;
    // Counted once nothing but the servers can refuse it: each try signs
    // in to them.
    let person = access.subject.user_id.to_string();
    if !state.limits.mail_sign_ins.allow(&person).await? {
        return Err(ApiError::RateLimited);
    }
    link_mail_account(&linking, prepared).await?;
    Ok(Json(Done::OK))
}

async fn remove(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    unlink_mail_account(&state.db, &access).await?;
    Ok(Json(Done::OK))
}
