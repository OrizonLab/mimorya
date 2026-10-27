//! Administration endpoints, each reserved to its permission. Reading the
//! directory and the organisation is open to members; managing them, and the
//! company-wide screens, require the permission on the whole company. A
//! refused change is audited with its reason (ADR-038), as is one aimed at a
//! resource answered as absent (ADR-047) or sending in its body an
//! identifier that designates nothing the company has (ADR-054).

mod absences;
mod activity;
mod approval_routing;
mod assignments;
mod connector_applications;
mod connectors;
mod embedding;
mod identity_providers;
mod llm;
mod memory_settings;
mod mission_settings;
mod organization;
mod policies;
mod roles;
mod skill_eval_cases;
mod skill_evaluations;
mod skills;
mod users;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch, post, put};

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
        .route(
            "/admin/users/{id}/absence",
            get(absences::show)
                .put(absences::declare)
                .delete(absences::end),
        )
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
            "/admin/approval-routing",
            get(approval_routing::show).put(approval_routing::update),
        )
        .route(
            "/admin/llm",
            get(llm::show).put(llm::configure).delete(llm::reset),
        )
        .route("/admin/llm/test", post(llm::test))
        .route(
            "/admin/llm/verification",
            put(llm::configure_verification).delete(llm::reset_verification),
        )
        .route("/admin/llm/verification/test", post(llm::test_verification))
        .route(
            "/admin/embedding",
            get(embedding::show)
                .put(embedding::configure)
                .delete(embedding::reset),
        )
        .route("/admin/embedding/test", post(embedding::test))
        .route(
            "/admin/memory-settings",
            get(memory_settings::show).put(memory_settings::update),
        )
        .route(
            "/admin/mission-settings",
            get(mission_settings::show)
                .put(mission_settings::update)
                .delete(mission_settings::reset),
        )
        .route(
            "/admin/connectors",
            get(connectors::list).post(connectors::create),
        )
        .route(
            "/admin/connectors/{id}",
            patch(connectors::update).delete(connectors::remove),
        )
        .route("/admin/connectors/trial", post(connectors::trial))
        .route("/admin/connectors/rest", post(connectors::create_rest))
        .route(
            "/admin/connectors/microsoft365",
            post(connector_applications::create_microsoft365),
        )
        .route(
            "/admin/connectors/google-workspace",
            post(connector_applications::create_google_workspace),
        )
        .route(
            "/admin/connectors/rest/import",
            post(connectors::import_rest),
        )
        .route(
            "/admin/identity-providers",
            get(identity_providers::list).post(identity_providers::create),
        )
        .route(
            "/admin/identity-providers/{id}",
            patch(identity_providers::update).delete(identity_providers::remove),
        )
        .route(
            "/admin/identity-providers/domain-proofs",
            post(identity_providers::prove_domain),
        )
        .route("/admin/connectors/{id}/test", post(connectors::test))
        .route("/admin/skills", get(skills::list).post(skills::create))
        .route(
            "/admin/skills/{id}",
            get(skills::show)
                .patch(skills::update)
                .delete(skills::remove),
        )
        .route("/admin/skills/{id}/versions", post(skills::add_version))
        .route(
            "/admin/skills/{id}/activation",
            put(skills::activate).delete(skills::retire),
        )
        .route("/admin/skills/{id}/rejection", post(skills::reject))
        .route("/admin/skills/{id}/cases", post(skill_eval_cases::create))
        .route(
            "/admin/skill-cases/{id}",
            put(skill_eval_cases::update).delete(skill_eval_cases::remove),
        )
        .route(
            "/admin/skills/{id}/evaluations",
            get(skill_evaluations::list).post(skill_evaluations::launch),
        )
        .route(
            "/admin/skill-evaluations/{id}",
            get(skill_evaluations::show),
        )
        .route("/admin/audit", get(activity::audit))
        .route("/admin/usage", get(activity::usage))
        // Last: a route layer only wraps the routes declared before it.
        .route_layer(from_fn_with_state(state.clone(), audit_refusals))
}
