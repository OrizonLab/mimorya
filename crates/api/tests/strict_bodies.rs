//! Request bodies are strict over HTTP (ADR-039), the administration's too
//! (ADR-040): a misnamed or extra field is answered 400, naming the field,
//! instead of being ignored while the request appears to succeed. Nothing is
//! changed.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_misnamed_or_extra_field_is_refused_and_changes_nothing() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;

    let misnamed = marc
        .post("/conversations", json!({ "titre": "Planning" }))
        .await;
    assert_eq!(misnamed.status, StatusCode::BAD_REQUEST);
    assert!(misnamed.text().contains("titre"), "{}", misnamed.text());

    let created = marc.post("/conversations", json!({})).await;
    assert_eq!(created.status, StatusCode::CREATED);
    let id = created.json()["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let url = format!("/conversations/{id}");
    // `archive` for `archived`: refused, the conversation stays as it was.
    let renamed = marc
        .patch(&url, json!({ "title": "Planning", "archive": true }))
        .await;
    assert_eq!(renamed.status, StatusCode::BAD_REQUEST);
    let detail = marc.get(&url).await.json();
    assert_ne!(detail["conversation"]["title"], "Planning");

    let memory = json!({
        "space": "personal",
        "kind": "preference",
        "title": "Rapports",
        "content": "Rapports en PDF",
        "sensitivity": "public",
    });
    assert_eq!(
        marc.post("/memory", memory).await.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn administration_bodies_are_strict_too() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = format!("/admin/users/{}", user::LUCAS);
    let before = support::accounts::directory_entry(&paul, user::LUCAS).await;

    // `jobtitle` for `jobTitle`: refused, the account stays as it was.
    let misnamed = paul
        .patch(
            &lucas,
            json!({ "displayName": "Lucas Essai", "jobtitle": "Chef" }),
        )
        .await;
    assert_eq!(misnamed.status, StatusCode::BAD_REQUEST);
    assert!(misnamed.text().contains("jobtitle"), "{}", misnamed.text());
    assert_eq!(
        support::accounts::directory_entry(&paul, user::LUCAS).await,
        before
    );

    let role = json!({ "name": "Rôle strict", "permissions": [], "scope": "tenant" });
    assert_eq!(
        paul.post("/admin/roles", role).await.status,
        StatusCode::BAD_REQUEST
    );

    // A unit given with the whole company: refused, nothing granted.
    let account = support::accounts::create_account(&paul, "portee.stricte").await;
    let grants = format!("/admin/users/{}/assignments", account.id);
    let held = support::accounts::directory_entry(&paul, account.id).await["assignments"].clone();
    let ambiguous = json!({
        "roleId": mimorya_core::demo::role::TEAM_LEAD,
        "scope": { "type": "tenant", "orgUnitId": mimorya_core::demo::unit::TEAM_SOUTH, "includeDescendants": false },
    });
    let granted = paul.post(&grants, ambiguous).await;
    assert_eq!(granted.status, StatusCode::BAD_REQUEST);
    assert!(granted.text().contains("orgUnitId"), "{}", granted.text());
    assert_eq!(
        support::accounts::directory_entry(&paul, account.id).await["assignments"],
        held
    );
}
