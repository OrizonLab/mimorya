//! Refused administration changes leave a trace with their reason, like the
//! changes made (ADR-038): on oneself, without the permission on the whole
//! company, on a platform role, without the permission at all. Reads,
//! absent or out-of-scope resources, invalid requests and anonymous
//! requests do not; nor does anything the request sent.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, tenant, user};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_account, create_partial_admin};
use uuid::Uuid;

/// Sent in refused requests: it must reach no entry.
const MARKER: &str = "Marqueur 7f3a";

#[derive(Debug)]
struct Refused {
    resource: Option<(String, String)>,
    reason: Option<String>,
    metadata: Value,
    request_id: Option<String>,
}

#[derive(sqlx::FromRow)]
struct RefusalRow {
    actor_user_id: Option<Uuid>,
    outcome: String,
    resource_type: Option<String>,
    resource_id: Option<String>,
    reason: Option<String>,
    metadata: Value,
    request_id: Option<String>,
}

/// The id of the latest audit entry, before the requests a test checks.
async fn latest_entry(api: &TestApi) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar("select coalesce(max(id), 0) from audit_log")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

/// The refusals audited after `since`, in order, for every user.
async fn refusals_since(api: &TestApi, since: i64) -> Vec<(Uuid, Refused)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<RefusalRow> = sqlx::query_as(
        "select actor_user_id, outcome, resource_type, resource_id, reason, metadata, \
         request_id from audit_log where action = 'admin.refused' and id > $1 order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows.into_iter()
        .map(|row| {
            assert_eq!(row.outcome, "denied");
            let refused = Refused {
                resource: row.resource_type.zip(row.resource_id),
                reason: row.reason,
                metadata: row.metadata,
                request_id: row.request_id,
            };
            (row.actor_user_id.expect("an actor"), refused)
        })
        .collect()
}

/// Every entry written after `since`, as text.
async fn entries_since(api: &TestApi, since: i64) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<Value> =
        sqlx::query_scalar("select to_jsonb(audit_log) from audit_log where id > $1 order by id")
            .bind(since)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    Value::from(rows).to_string()
}

fn route(method: &str, route: &str) -> Value {
    json!({ "method": method, "route": route })
}

fn on(resource_type: &str, id: Uuid) -> Option<(String, String)> {
    Some((resource_type.to_string(), id.to_string()))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refused_changes_on_oneself_and_on_platform_roles_are_audited_with_their_reason() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let own = format!("/admin/users/{}", user::PAUL);

    let renamed = paul.patch(&own, json!({ "displayName": MARKER })).await;
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
        .filter(|e| e["action"] == "admin.refused" && e["outcome"] == "denied")
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
            json!({ "parentId": mimorya_core::demo::unit::TEAM_NORTH, "name": MARKER, "type": "team" }),
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
