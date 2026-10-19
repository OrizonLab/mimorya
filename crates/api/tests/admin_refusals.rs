//! Refused administration changes leave a trace with their reason, like the
//! changes made (ADR-038): on oneself, without the permission on the whole
//! company, on a platform role, without the permission at all, before the
//! temporary password is replaced. Reads, resources an authorised
//! administrator finds absent or out of scope, invalid requests and
//! anonymous requests do not; nor does anything the request sent but the
//! path's identifier. A refusal stands when its trace cannot be written.

mod support;

use axum::http::StatusCode;
use mimorya_api::plugins::refusal_audit::REFUSED_ACTION;
use mimorya_core::demo::{role, unit, user};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_account, create_partial_admin};
use support::mcp::capture_logs;
use support::privileges::without_insert;
use support::refusals::{entries_since, latest_entry, refusals_since};
use uuid::Uuid;

/// Sent in refused requests: it must reach no entry.
const MARKER: &str = "Marqueur 7f3a";
/// The client's address, as a trusted proxy forwards it.
const CLIENT: &str = "203.0.113.7";

fn route(method: &str, route: &str) -> Value {
    json!({ "method": method, "route": route })
}

fn on(resource_type: &str, id: Uuid) -> Option<(String, String)> {
    Some((resource_type.to_string(), id.to_string()))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refused_changes_on_oneself_and_on_platform_roles_are_audited_with_their_reason() {
    let api = TestApi::start_with(&[("TRUST_PROXY", "true")]).await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let own = format!("/admin/users/{}", user::PAUL);

    let renamed = paul
        .patch_with(
            &own,
            json!({ "displayName": MARKER }),
            &[("x-forwarded-for", CLIENT)],
        )
        .await;
    assert_eq!(renamed.status, StatusCode::FORBIDDEN);
    let reset = paul.post(&format!("{own}/password"), json!({})).await;
    assert_eq!(reset.status, StatusCode::FORBIDDEN);
    let platform_role = format!("/admin/roles/{}", role::TENANT_ADMIN);
    let edited = paul
        .patch(&platform_role, json!({ "description": MARKER }))
        .await;
    assert_eq!(edited.status, StatusCode::FORBIDDEN);

    let refused = refusals_since(&api, since).await;
    assert_eq!(refused.len(), 3, "{refused:?}");
    assert!(refused.iter().all(|(actor, _)| *actor == user::PAUL));
    let expected = [
        (
            renamed,
            on("user", user::PAUL),
            route("PATCH", "/admin/users/{id}"),
        ),
        (
            reset,
            on("user", user::PAUL),
            route("POST", "/admin/users/{id}/password"),
        ),
        (
            edited,
            on("role", role::TENANT_ADMIN),
            route("PATCH", "/admin/roles/{id}"),
        ),
    ];
    for ((_, entry), (response, resource, metadata)) in refused.iter().zip(expected) {
        assert_eq!(
            entry.reason.as_deref(),
            Some(response.error_message().as_str())
        );
        assert_eq!(entry.resource, resource);
        assert_eq!(entry.metadata, metadata);
        assert!(entry.request_id.is_some());
    }
    // The address the trusted proxy forwarded, for the request that had one.
    let addresses: Vec<Option<&str>> = refused.iter().map(|(_, e)| e.ip.as_deref()).collect();
    assert_eq!(addresses, [Some(CLIENT), None, None]);
    assert!(
        refused[0]
            .1
            .reason
            .as_deref()
            .is_some_and(|r| r.contains("votre propre compte"))
    );
    assert!(!entries_since(&api, since).await.contains(MARKER));

    // The administration's audit screen shows them.
    let audit = paul.get("/admin/audit").await;
    assert_eq!(audit.status, StatusCode::OK);
    let shown = audit.json()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["action"] == REFUSED_ACTION && e["outcome"] == "denied")
        .count();
    assert!(shown >= 3);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refusals_for_lack_of_permission_name_the_permission() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let mut account = create_partial_admin(&api, &paul, "admin.refuse").await;
    let partial = activate(&api, &mut account).await;
    let since = latest_entry(&api).await;

    // An administrator whose role only covers one team.
    let lucas = format!("/admin/users/{}", user::LUCAS);
    let disabled = partial.patch(&lucas, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::FORBIDDEN);
    let unit = partial
        .post(
            "/admin/org-units",
            json!({ "parentId": unit::TEAM_NORTH, "name": MARKER, "type": "team" }),
        )
        .await;
    assert_eq!(unit.status, StatusCode::FORBIDDEN);
    // A member without any administration role.
    let granted = marc
        .post(
            &format!("{lucas}/assignments"),
            json!({ "roleId": role::DIRECTOR, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::FORBIDDEN);

    let refused = refusals_since(&api, since).await;
    assert_eq!(refused.len(), 3, "{refused:?}");
    let expected = [
        (
            account.id,
            disabled,
            on("user", user::LUCAS),
            route("PATCH", "/admin/users/{id}"),
        ),
        (account.id, unit, None, route("POST", "/admin/org-units")),
        (
            user::MARC,
            granted,
            on("user", user::LUCAS),
            route("POST", "/admin/users/{id}/assignments"),
        ),
    ];
    for ((actor, entry), (by, response, resource, metadata)) in refused.iter().zip(expected) {
        assert_eq!(*actor, by);
        let reason = response.error_message();
        assert!(reason.starts_with("Permission requise"), "{reason}");
        assert_eq!(entry.reason.as_deref(), Some(reason.as_str()));
        assert_eq!(entry.resource, resource);
        assert_eq!(entry.metadata, metadata);
    }
    assert!(!entries_since(&api, since).await.contains(MARKER));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_refused_changes_are_audited_as_refusals() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let mut account = create_partial_admin(&api, &paul, "admin.lecture").await;
    let partial = activate(&api, &mut account).await;
    let since = latest_entry(&api).await;

    // A change made.
    let created = create_account(&paul, "compte.accepte").await;
    // A read refused.
    assert_eq!(
        partial.get("/admin/audit").await.status,
        StatusCode::FORBIDDEN
    );
    // Out of scope: another company's account, answered as absent.
    let elsewhere = paul
        .post(
            &format!("/admin/users/{OTHER_USER_ID}/assignments"),
            json!({ "roleId": role::DIRECTOR, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(elsewhere.status, StatusCode::NOT_FOUND);
    // An invalid request.
    let expired = paul
        .post(
            &format!("/admin/users/{}/assignments", created.id),
            json!({
                "roleId": role::DIRECTOR,
                "scope": { "type": "tenant" },
                "expiresAt": "2020-01-01T00:00:00Z",
            }),
        )
        .await;
    assert_eq!(expired.status, StatusCode::BAD_REQUEST);
    // Anonymous.
    let anonymous = api
        .anonymous()
        .patch(
            &format!("/admin/users/{}", user::LUCAS),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

    assert!(refusals_since(&api, since).await.is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refusals_before_the_temporary_password_is_replaced_are_audited() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "mot.de.passe.temporaire").await;
    let pending = api.sign_in_with(&account.email, &account.password).await;
    let since = latest_entry(&api).await;

    let disabled = pending
        .patch(
            &format!("/admin/users/{}", user::LUCAS),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::FORBIDDEN);

    let refused = refusals_since(&api, since).await;
    assert_eq!(refused.len(), 1, "{refused:?}");
    let (actor, entry) = &refused[0];
    assert_eq!(*actor, account.id);
    assert_eq!(
        entry.reason.as_deref(),
        Some(disabled.error_message().as_str())
    );
    assert!(disabled.error_message().contains("mot de passe"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_refusal_stands_when_its_trace_cannot_be_written() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let logs = capture_logs();

    let own = format!("/admin/users/{}", user::PAUL);
    let disabled = without_insert(
        "audit_log",
        paul.patch(&own, json!({ "status": "disabled" })),
    )
    .await;

    assert_eq!(disabled.status, StatusCode::FORBIDDEN);
    assert!(disabled.error_message().contains("votre propre compte"));
    assert!(refusals_since(&api, since).await.is_empty());
    assert!(
        logs.text()
            .contains("Failed to audit a refused administration change")
    );
}
