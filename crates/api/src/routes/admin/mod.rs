//! Administration endpoints, each reserved to its permission. Reading the
//! directory and the organisation is open to members; managing them, and the
//! company-wide screens, require the permission on the whole company. A
//! refused change is audited with its reason (ADR-038), as is one aimed at a
//! resource answered as absent (ADR-047).

mod activity;
mod assignments;
mod catalog;
mod connectors;
mod llm;
mod organization;
mod policies;
mod roles;
mod users;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch, post};

use crate::app::AppState;
use crate::plugins::refusal_audit::audit_refusals;

pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/organization", get(organization::organization))
        .route("/admin/org-units", post(organization::create))
        .route(
            "/admin/org-units/{id}",
            patch(organization::update).delete(organization::remove),
        )
        .route("/admin/users", get(users::list).post(users::create))
        .route("/admin/users/{id}", patch(users::update))
        .route("/admin/users/{id}/password", post(users::reset))
        .route("/admin/users/{id}/password-link", post(users::send_link))
        .route("/admin/users/{id}/invitation", post(users::invite))
        .route("/admin/users/{id}/assignments", post(assignments::grant))
        .route("/admin/assignments/{id}", delete(assignments::revoke))
        .route("/admin/roles", get(roles::list).post(roles::create))
        .route(
            "/admin/roles/{id}",
            patch(roles::update).delete(roles::remove),
        )
        .route(
            "/admin/policies",
            get(policies::list).post(policies::create),
        )
        .route(
            "/admin/policies/{id}",
            patch(policies::update).delete(policies::remove),
        )
        .route(
            "/admin/llm",
            get(llm::show).put(llm::configure).delete(llm::reset),
        )
        .route("/admin/llm/test", post(llm::test))
        .route(
            "/admin/connectors",
            get(connectors::list).post(connectors::create),
        )
        .route(
            "/admin/connectors/{id}",
            patch(connectors::update).delete(connectors::remove),
        )
        .route("/admin/connectors/trial", post(connectors::trial))
        .route("/admin/connectors/{id}/test", post(connectors::test))
        .route("/admin/skills", get(catalog::skills))
        .route("/admin/audit", get(activity::audit))
        .route("/admin/usage", get(activity::usage))
        // Last: a route layer only wraps the routes declared before it.
        .route_layer(from_fn_with_state(state.clone(), audit_refusals))
}
