//! Refused administration changes are audited with their reason, like the
//! changes made (ADR-038): one aimed at the administrator's own account, one
//! needing a permission they do not hold on the whole company, one on a
//! platform role… Layered on the administration routes. Reads are not
//! concerned, nor answers that a resource is absent or out of scope (they
//! are indistinguishable on purpose). The reason is the refusal's message,
//! written by the platform: it never carries what the request sent.

use axum::RequestExt;
use axum::extract::{MatchedPath, RawPathParams, Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;
use uuid::Uuid;

use crate::app::AppState;
use crate::http::Refusal;
use crate::plugins::RequestContext;
use crate::services::AuthIdentity;

pub const REFUSED_ACTION: &str = "admin.refused";

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
    if let Some(Refusal(reason)) = response.extensions().get::<Refusal>() {
        // The refusal stands whether or not its trace is written.
        if let Err(error) = record(&state, attempt, reason).await {
            tracing::error!(
                error = format!("{error:#}"),
                "Failed to audit a refused administration change"
            );
        }
    }
    response
}

async fn record(state: &AppState, attempt: Attempt, reason: &str) -> anyhow::Result<()> {
    let tenant_id = attempt.identity.tenant_id;
    let mut entry = AuditEntry::new(tenant_id, "user", REFUSED_ACTION, "denied")
        .by_user(attempt.identity.user_id)
        .reason(reason)
        .metadata(json!({ "method": attempt.method.as_str(), "route": attempt.route }));
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
/// path is what the request sent.
fn resource_of(route: &str, id: &str) -> Option<(&'static str, Uuid)> {
    let id = id.parse().ok()?;
    let collection = route.strip_prefix("/admin/")?.split('/').next()?;
    let resource_type = match collection {
        "org-units" => "org_unit",
        "users" => "user",
        "assignments" => "assignment",
        "roles" => "role",
        "policies" => "policy",
        "connectors" => "connector",
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
            ("/admin/connectors/{id}", "connector"),
            ("/admin/connectors/{id}/test", "connector"),
        ] {
            assert_eq!(resource_of(route, ID), Some((resource_type, id)), "{route}");
        }
    }

    #[test]
    fn keeps_only_an_identifier() {
        assert_eq!(resource_of("/admin/users/{id}", "<script>"), None);
        assert_eq!(resource_of("/admin/users/{id}", &format!("{ID}x")), None);
        assert_eq!(resource_of("/elsewhere/{id}", ID), None);
    }
}
