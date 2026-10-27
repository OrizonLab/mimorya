//! A company's Google Workspace client, declared by its connector
//! administrators (ADR-121): the OAuth client ID checked, the secret
//! sealed, never returned nor audited, one per company, beside its
//! Microsoft 365 application; the address to register shown with how many
//! people linked their account; its secret replaced, never removed; its
//! addresses Google's; whether its tools may write, changed.

mod support;

use axum::http::StatusCode;
use mimorya_core::account_links::{GOOGLE_CALENDAR, GOOGLE_GMAIL};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::account_links::DIRECTORY;
use support::refusals::latest_entry;
use support::sso::{SSO_DEPLOYMENT, audit_text_since};
use support::{Client, TestApi, WEB_ORIGIN};

const DECLARE: &str = "/admin/connectors/google-workspace";
const CLIENT: &str = "123456789012-abcdef0123456789.apps.googleusercontent.com";
const SECRET: &str = "GOCSPX-Secret_Client.2026";

fn declaration() -> Value {
    json!({
        "name": "Google Workspace",
        "clientId": CLIENT,
        "clientSecret": SECRET,
    })
}

async fn google_view(paul: &Client<'_>) -> Value {
    let listed = paul.get("/admin/connectors").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    let body = listed.json();
    let found = body["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "google_workspace")
        .cloned();
    found.unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_declares_its_google_client_once() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let types = paul.get("/admin/connectors").await.json()["types"].clone();
    assert!(
        types
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["type"] == "google_workspace"),
        "{types}"
    );

    for (field, value, said) in [
        ("clientId", "mimorya", "apps.googleusercontent.com"),
        (
            "clientId",
            "1234.apps.googleusercontent.com.evil.example",
            "apps.googleusercontent.com",
        ),
        ("clientSecret", "deux mots", "secret client"),
    ] {
        let mut body = declaration();
        body[field] = json!(value);
        let refused = paul.post(DECLARE, body).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        assert!(refused.error_message().contains(said), "{}", refused.text());
    }
    let mut unknown = declaration();
    unknown["directory"] = json!(DIRECTORY);
    let strict = paul.post(DECLARE, unknown).await;
    assert_eq!(strict.status, StatusCode::BAD_REQUEST, "{}", strict.text());
    let claire = api.sign_in(user::CLAIRE).await;
    let forbidden = claire.post(DECLARE, declaration()).await;
    assert_eq!(
        forbidden.status,
        StatusCode::FORBIDDEN,
        "{}",
        forbidden.text()
    );

    let since = latest_entry(&api).await;
    let created = paul.post(DECLARE, declaration()).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_string();
    // Stored with Google's issuer, Gmail and Calendar.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let config: Value =
        sqlx::query_scalar("select config from connector_instances where id = $1::uuid")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        config,
        json!({
            "clientId": CLIENT,
            "issuer": "https://accounts.google.com",
            "gmail": GOOGLE_GMAIL,
            "calendar": GOOGLE_CALENDAR,
            "writeTools": false,
        })
    );
    let view = google_view(&paul).await;
    assert_eq!(
        view["googleWorkspace"],
        json!({
            "clientId": CLIENT,
            "redirectUri": format!("{WEB_ORIGIN}/api/account/links/callback"),
            "secretStored": true,
            "links": 0,
            "writeTools": false,
        })
    );
    assert_eq!(view["microsoft365"], Value::Null);
    assert!(!view.to_string().contains(SECRET));
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(audited.contains(CLIENT), "{audited}");
    assert!(!audited.contains(SECRET), "{audited}");

    // One per company, whatever its name; a Microsoft 365 application
    // beside it.
    let mut other = declaration();
    other["name"] = json!("Autre client");
    let twice = paul.post(DECLARE, other).await;
    assert_eq!(twice.status, StatusCode::CONFLICT, "{}", twice.text());
    let microsoft = paul
        .post(
            "/admin/connectors/microsoft365",
            json!({
                "name": "Microsoft 365",
                "directory": DIRECTORY,
                "clientId": "0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0",
                "clientSecret": SECRET,
            }),
        )
        .await;
    assert_eq!(
        microsoft.status,
        StatusCode::CREATED,
        "{}",
        microsoft.text()
    );

    for created in [id, microsoft.json()["id"].as_str().unwrap().to_string()] {
        let removed = paul.delete(&format!("/admin/connectors/{created}")).await;
        assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    }
    assert_eq!(google_view(&paul).await, Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn its_secret_is_replaced_never_removed_and_its_addresses_stay() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let created = paul.post(DECLARE, declaration()).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let path = format!(
        "/admin/connectors/{}",
        created.json()["id"].as_str().unwrap()
    );

    let refusals = [
        (json!({ "token": null }), "ne se retire pas"),
        (
            json!({ "url": "https://gmail.example/gmail/v1" }),
            "Les adresses de Google Workspace sont celles de la plateforme",
        ),
        (
            json!({ "toolPolicies": { "google_gmail_search": { "enabled": false } } }),
            "Outil inconnu de ce connecteur",
        ),
        (json!({ "tokenHeader": "X-Api-Key" }), "Seule une API REST"),
    ];
    for (body, said) in refusals {
        let refused = paul.patch(&path, body).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        assert!(refused.error_message().contains(said), "{}", refused.text());
    }

    let since = latest_entry(&api).await;
    let replaced = paul
        .patch(
            &path,
            json!({ "token": "GOCSPX-Nouveau.2027", "name": "Gmail", "writeTools": true }),
        )
        .await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.text());
    let view = google_view(&paul).await;
    assert_eq!(view["name"], json!("Gmail"));
    assert_eq!(view["googleWorkspace"]["secretStored"], json!(true));
    assert_eq!(view["googleWorkspace"]["writeTools"], json!(true));
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(
        audited.contains("replaced") && !audited.contains("GOCSPX-Nouveau"),
        "{audited}"
    );

    let removed = paul.delete(&path).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}
