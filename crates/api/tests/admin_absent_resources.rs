//! Administration changes aimed at a resource answered as absent leave a
//! trace, like the refused ones (ADR-047): the resource may be absent or
//! another company's, which neither the answer nor the entry tells apart, and
//! the entry is written in the administrator's company only. Only an
//! administrator holding the permission reaches that answer: the others are
//! refused first (ADR-038). Answers of absence that designate no resource
//! leave none, nor does anything the request sent but the path's identifier;
//! unknown identifiers sent in the body (answered as invalid) are traced
//! apart (ADR-054, `admin_unknown_references.rs`).
//! An absence stands when its trace cannot be written.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, user};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::mcp::capture_logs;
use support::privileges::without_insert;
use support::refusals::{
    entries_of_since, entries_since, latest_entry, latest_entry_of, not_found_since, refusals_since,
};
use uuid::Uuid;

/// Sent in the requests: it must reach no entry.
const MARKER: &str = "Marqueur 5c1e";

fn route(method: &str, route: &str) -> Value {
    json!({ "method": method, "route": route })
}

/// A custom role of the other company.
async fn foreign_role(api: &TestApi) -> Uuid {
    let id = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query(
        "insert into roles (id, tenant_id, key, name, permissions) \
         values ($1, $2, 'custom_absent', 'Absent', '{intervention:read}')",
    )
    .bind(id)
    .bind(OTHER_TENANT_ID)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn changes_on_resources_answered_as_absent_are_audited_alike() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let foreign = foreign_role(&api).await;
    let since = latest_entry(&api).await;
    let elsewhere = latest_entry_of(&api, OTHER_TENANT_ID).await;

    // Each change twice: on another company's resource, then on one that
    // exists nowhere. Both are answered alike.
    let renamed = json!({ "displayName": MARKER });
    let mut requests = Vec::new();
    for id in [OTHER_USER_ID, Uuid::new_v4()] {
        let url = format!("/admin/users/{id}");
        requests.push((paul.patch(&url, renamed.clone()).await, "user", id, "PATCH"));
    }
    for id in [foreign, Uuid::new_v4()] {
        let url = format!("/admin/roles/{id}");
        requests.push((paul.delete(&url).await, "role", id, "DELETE"));
    }
    for (pair, routes) in requests
        .chunks(2)
        .zip(["/admin/users/{id}", "/admin/roles/{id}"])
    {
        let [(foreign, ..), (absent, ..)] = pair else {
            unreachable!()
        };
        assert_eq!(foreign.status, StatusCode::NOT_FOUND, "{routes}");
        assert_eq!(foreign.json(), absent.json(), "{routes}");
    }

    let traced = not_found_since(&api, since).await;
    assert_eq!(traced.len(), 4, "{traced:?}");
    let expected = [
        "/admin/users/{id}",
        "/admin/users/{id}",
        "/admin/roles/{id}",
        "/admin/roles/{id}",
    ];
    for (((actor, entry), (response, resource_type, id, method)), path) in
        traced.iter().zip(&requests).zip(expected)
    {
        assert_eq!(*actor, user::PAUL);
        let resource = Some((resource_type.to_string(), id.to_string()));
        assert_eq!(entry.resource, resource);
        assert_eq!(
            entry.reason.as_deref(),
            Some(response.error_message().as_str())
        );
        assert_eq!(entry.metadata, route(method, path));
        assert!(entry.request_id.is_some());
    }
    // Apart from the identifier, another company's resource and an absent
    // one leave the same entry.
    for pair in traced.chunks(2) {
        let (foreign, absent) = (&pair[0].1, &pair[1].1);
        assert_eq!(
            (&foreign.reason, &foreign.metadata, &foreign.ip),
            (&absent.reason, &absent.metadata, &absent.ip)
        );
    }
    assert!(refusals_since(&api, since).await.is_empty());
    assert!(!entries_since(&api, since).await.contains(MARKER));
    // Nothing is written in the other company's audit.
    assert_eq!(
        entries_of_since(&api, OTHER_TENANT_ID, elsewhere).await,
        "[]"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_resource_the_administration_changes_is_traced_when_absent() {
    // With a mail relay, the reset link reaches the account's lookup.
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let absent = Uuid::new_v4();

    let grant = json!({ "roleId": role::DIRECTOR, "scope": { "type": "tenant" } });
    let answers = [
        (
            paul.post(&format!("/admin/users/{absent}/password"), json!({}))
                .await,
            "user",
        ),
        (
            paul.post(&format!("/admin/users/{absent}/password-link"), json!({}))
                .await,
            "user",
        ),
        (
            paul.post(&format!("/admin/users/{absent}/invitation"), json!({}))
                .await,
            "user",
        ),
        (
            paul.post(&format!("/admin/users/{absent}/assignments"), grant)
                .await,
            "user",
        ),
        (
            paul.delete(&format!("/admin/assignments/{absent}")).await,
            "assignment",
        ),
        (
            paul.patch(
                &format!("/admin/org-units/{absent}"),
                json!({ "name": MARKER }),
            )
            .await,
            "org_unit",
        ),
        (
            paul.delete(&format!("/admin/policies/{absent}")).await,
            "policy",
        ),
        (
            paul.patch(
                &format!("/admin/connectors/{absent}"),
                json!({ "name": MARKER }),
            )
            .await,
            "connector",
        ),
        (
            paul.post(&format!("/admin/connectors/{absent}/test"), json!({}))
                .await,
            "connector",
        ),
    ];
    for (response, resource_type) in &answers {
        assert_eq!(response.status, StatusCode::NOT_FOUND, "{resource_type}");
    }

    let traced = not_found_since(&api, since).await;
    let resources: Vec<_> = traced.iter().map(|(_, e)| e.resource.clone()).collect();
    let expected: Vec<_> = answers
        .iter()
        .map(|(_, resource_type)| Some((resource_type.to_string(), absent.to_string())))
        .collect();
    assert_eq!(resources, expected);
    assert!(!entries_since(&api, since).await.contains(MARKER));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_an_authorised_administrator_and_the_path_resource_are_traced() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let mut account = create_partial_admin(&api, &paul, "admin.absent").await;
    let partial = activate(&api, &mut account).await;
    let since = latest_entry(&api).await;

    // Another company's account: refused, as any account would be, to an
    // administrator limited to a team and to a member.
    let url = format!("/admin/users/{OTHER_USER_ID}");
    for client in [&partial, &marc] {
        let disabled = client.patch(&url, json!({ "status": "disabled" })).await;
        assert_eq!(disabled.status, StatusCode::FORBIDDEN);
    }
    // An answer of absence that designates no resource: the company already
    // uses the platform's model.
    assert_eq!(
        paul.delete("/admin/llm").await.status,
        StatusCode::NOT_FOUND
    );
    // An unknown role sent in the body: answered as invalid.
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", user::LUCAS),
            json!({ "roleId": Uuid::new_v4(), "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::BAD_REQUEST);

    assert_eq!(refusals_since(&api, since).await.len(), 2);
    assert!(not_found_since(&api, since).await.is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_absence_stands_when_its_trace_cannot_be_written() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let logs = capture_logs();

    let url = format!("/admin/users/{OTHER_USER_ID}");
    let disabled = without_insert(
        "audit_log",
        paul.patch(&url, json!({ "status": "disabled" })),
    )
    .await;

    assert_eq!(disabled.status, StatusCode::NOT_FOUND);
    assert_eq!(disabled.error_message(), "Compte introuvable");
    assert!(not_found_since(&api, since).await.is_empty());
    assert!(
        logs.text()
            .contains("Failed to audit a refused administration change")
    );
}
