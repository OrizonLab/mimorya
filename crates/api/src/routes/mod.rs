//! HTTP routes, one module per resource. Every route resolves the caller with
//! `Auth`, builds the authorization context server-side and checks the
//! permission or the ownership of the resource before answering. Exceptions:
//! login, invitation and password reset links and health need no identity,
//! nor a delivery to an inbound hook, proved by its token (ADR-109), nor an
//! event Slack sends a gateway, proved by its signature (ADR-125);
//! logout takes `MaybeAuth`; `/auth/me`
//! and `/account/password` take `AuthPendingPasswordChange`, so that an
//! account holding a temporary password can replace it.

mod account;
mod account_gateways;
mod account_links;
mod admin;
mod approval_delegation;
mod approvals;
mod auth;
mod conversations;
mod documents;
mod gateways;
mod health;
mod hooks;
mod inbound_hooks;
mod invitations;
mod memory;
mod missions;
mod password_reset;
mod run_events;
mod runs;
mod saml;
mod scheduled_tasks;
mod sso;

use axum::Router;
use mimorya_core::DomainError;

use crate::app::AppState;
use crate::http::ApiError;

pub use gateways::{SLACK_EVENTS_PREFIX, TEAMS_ACTIVITIES_PREFIX, TELEGRAM_UPDATES_PREFIX};
pub use hooks::DELIVERY_PREFIX as HOOK_DELIVERY_PREFIX;
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
        .merge(account_links::routes())
        .merge(account_gateways::routes())
        .merge(conversations::routes())
        .merge(runs::routes())
        .merge(run_events::routes())
        .merge(approvals::routes())
        .merge(approval_delegation::routes())
        .merge(memory::routes())
        .merge(documents::routes())
        .merge(scheduled_tasks::routes())
        .merge(inbound_hooks::routes())
        .merge(hooks::routes())
        .merge(gateways::routes())
        .merge(missions::routes())
        .merge(admin::routes(state))
        .fallback(not_found)
}

async fn not_found() -> ApiError {
    DomainError::not_found("Ressource introuvable").into()
}
