//! Refused administration changes are audited with their reason, like the
//! changes made (ADR-038): one aimed at the administrator's own account, one
//! needing a permission they do not hold on the whole company, one on a
//! platform role… So are the changes aimed at a resource the route designates
//! and answered as absent (ADR-047): it may be absent, or another company's,
//! which the entry does not tell apart any more than the answer does. So are
//! the changes that send in their body an identifier designating nothing the
//! company has, answered as invalid (ADR-054): the entry keeps that
//! identifier and what it was meant to designate, alike for an absent and a
//! foreign one. Layered on the administration routes; reads are not
//! concerned. The reason is the answer's message, written by the platform:
//! besides the identifiers, it never carries what the request sent.

use axum::RequestExt;
use axum::extract::{MatchedPath, RawPathParams, Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;
use uuid::Uuid;

use mimorya_core::UnknownReference;

use crate::app::AppState;
use crate::http::Refusal;
use crate::plugins::RequestContext;
use crate::services::AuthIdentity;

pub const REFUSED_ACTION: &str = "admin.refused";
/// A change aimed at a resource answered as absent (ADR-047).
pub const NOT_FOUND_ACTION: &str = "admin.not_found";
/// A change sending an identifier that designates nothing (ADR-054).
pub const UNKNOWN_REFERENCE_ACTION: &str = "admin.unknown_reference";

/// A change attempted by a signed-in user, as the refusal's entry names it.
struct Attempt {
    identity: AuthIdentity,
    context: Option<RequestContext>,
    method: Method,
    route: String,
    resource: Option<(&'static str, Uuid)>,
}

pub async fn audit_refusals(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let identity = request.extensions().get::<AuthIdentity>().copied();
    let (Some(identity), false) = (identity, request.method().is_safe()) else {
        return next.run(request).await;
    };
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_default();
    let resource = match request.extract_parts::<RawPathParams>().await {
        Ok(params) => {
            let id = params.iter().find(|(name, _)| *name == "id");
            id.and_then(|(_, id)| resource_of(&route, id))
        }
        Err(_) => None,
    };
    let attempt = Attempt {
        identity,
        context: request.extensions().get::<RequestContext>().copied(),
        method: request.method().clone(),
        route,
        resource,
    };
    let response = next.run(request).await;
    let traced = match response.extensions().get::<Refusal>() {
        Some(Refusal::Forbidden(reason)) => Some((REFUSED_ACTION, reason, None)),
        // Only the resource the route designates: the other answers of
        // absence (the company's own model, already the platform's) are no
        // attempt on someone else's resource.
        Some(Refusal::NotFound(reason)) if attempt.resource.is_some() => {
            Some((NOT_FOUND_ACTION, reason, None))
        }
        Some(Refusal::UnknownReference(reason, reference)) => {
            Some((UNKNOWN_REFERENCE_ACTION, reason, Some(*reference)))
        }
        _ => None,
    };
    if let Some((action, reason, reference)) = traced {
        // The answer stands whether or not its trace is written.
        if let Err(error) = record(&state, attempt, action, reason, reference).await {
            tracing::error!(
                error = format!("{error:#}"),
                action,
                "Failed to audit a refused administration change"
            );
        }
    }
    response
}

async fn record(
    state: &AppState,
    attempt: Attempt,
    action: &str,
    reason: &str,
    reference: Option<UnknownReference>,
) -> anyhow::Result<()> {
    let tenant_id = attempt.identity.tenant_id;
    let mut metadata = json!({ "method": attempt.method.as_str(), "route": attempt.route });
    if let Some(reference) = reference {
        metadata["reference"] = json!({ "type": reference.resource_type, "id": reference.id });
    }
    let mut entry = AuditEntry::new(tenant_id, "user", action, "denied")
        .by_user(attempt.identity.user_id)
        .reason(reason)
        .metadata(metadata);
    if let Some((resource_type, id)) = attempt.resource {
        entry = entry.resource(resource_type, id);
    }
    if let Some(context) = attempt.context {
        entry = entry.ip(context.ip_string());
        entry.request_id = Some(context.id.to_string());
    }
    let mut tx = state.db.begin(tenant_id).await?;
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// The resource a route's `{id}` designates, named as the audit of the
/// changes made names it. Only an identifier is kept: anything else in the
/// path is what the request sent. A collection left unnamed here leaves its
/// refusals without resource and its absent resources untraced: a test
/// reads the administration's router for that. An absence answered on such
/// a route is taken as the path's resource: an unknown identifier sent in
/// the body is answered as invalid, not as absent (ADR-047), and traced as
/// such (ADR-054).
fn resource_of(route: &str, id: &str) -> Option<(&'static str, Uuid)> {
    let id = id.parse().ok()?;
    let collection = route.strip_prefix("/admin/")?.split('/').next()?;
    let resource_type = match collection {
        "org-units" => "org_unit",
        "users" => "user",
        "assignments" => "assignment",
        "roles" => "role",
        "policies" => "policy",
        "knowledge-shares" => "knowledge_share",
        "connectors" => "connector",
        "identity-providers" => "identity_provider",
        "gateways" => "gateway",
        "skills" => "skill",
        "skill-cases" => "skill_eval_case",
        "skill-evaluations" => "skill_evaluation",
        _ => return None,
    };
    Some((resource_type, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "8d5ba3d6-0c4c-4b2c-9c51-2f8e6a3c1b7e";

    #[test]
    fn names_the_resource_of_every_administration_route_with_an_id() {
        let id: Uuid = ID.parse().unwrap();
        for (route, resource_type) in [
            ("/admin/org-units/{id}", "org_unit"),
            ("/admin/users/{id}", "user"),
            ("/admin/users/{id}/password", "user"),
            ("/admin/users/{id}/password-link", "user"),
            ("/admin/users/{id}/invitation", "user"),
            ("/admin/users/{id}/assignments", "user"),
            ("/admin/assignments/{id}", "assignment"),
            ("/admin/roles/{id}", "role"),
            ("/admin/policies/{id}", "policy"),
            ("/admin/knowledge-shares/{id}", "knowledge_share"),
            ("/admin/connectors/{id}", "connector"),
            ("/admin/connectors/{id}/test", "connector"),
            ("/admin/identity-providers/{id}", "identity_provider"),
            ("/admin/gateways/{id}", "gateway"),
            ("/admin/skills/{id}", "skill"),
            ("/admin/skills/{id}/versions", "skill"),
            ("/admin/skills/{id}/activation", "skill"),
            ("/admin/skills/{id}/rejection", "skill"),
            ("/admin/skills/{id}/cases", "skill"),
            ("/admin/skill-cases/{id}", "skill_eval_case"),
            ("/admin/skills/{id}/evaluations", "skill"),
            ("/admin/skill-evaluations/{id}", "skill_evaluation"),
        ] {
            assert_eq!(resource_of(route, ID), Some((resource_type, id)), "{route}");
        }
    }

    #[test]
    fn names_every_route_with_an_id_the_administration_declares() {
        let router = include_str!("../routes/admin/mod.rs");
        // The quoted paths of the router's source.
        let routes: Vec<&str> = router
            .split('"')
            .filter(|s| s.starts_with("/admin/") && s.contains("{id}"))
            .collect();
        assert!(routes.len() >= 14, "{routes:?}");
        for route in routes {
            assert!(resource_of(route, ID).is_some(), "{route}");
        }
    }

    #[test]
    fn keeps_only_an_identifier() {
        assert_eq!(resource_of("/admin/users/{id}", "<script>"), None);
        assert_eq!(resource_of("/admin/users/{id}", &format!("{ID}x")), None);
        assert_eq!(resource_of("/elsewhere/{id}", ID), None);
    }
}
