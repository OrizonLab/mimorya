//! HTTP routes, one module per resource. Every route resolves the caller with
//! `Auth`, builds the authorization context server-side and checks the
//! permission or the ownership of the resource before answering. Exceptions:
//! login, invitation and password reset links and health need no identity;
//! logout takes `MaybeAuth`; `/auth/me`
//! and `/account/password` take `AuthPendingPasswordChange`, so that an
//! account holding a temporary password can replace it.

mod account;
mod admin;
mod approval_delegation;
mod approvals;
mod auth;
mod conversations;
mod health;
mod invitations;
mod memory;
mod password_reset;
mod run_events;
mod runs;
mod saml;
mod sso;

use axum::Router;
use mimorya_core::DomainError;

use crate::app::AppState;
use crate::http::ApiError;

pub use saml::ACS_ROUTE as SAML_ACS_ROUTE;

pub fn router(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(auth::routes())
        .merge(sso::routes())
        .merge(saml::routes())
        .merge(invitations::routes())
        .merge(password_reset::routes())
        .merge(account::routes())
        .merge(conversations::routes())
        .merge(runs::routes())
        .merge(run_events::routes())
        .merge(approvals::routes())
        .merge(approval_delegation::routes())
        .merge(memory::routes())
        .merge(admin::routes(state))
        .fallback(not_found)
}

async fn not_found() -> ApiError {
    DomainError::not_found("Ressource introuvable").into()
}
