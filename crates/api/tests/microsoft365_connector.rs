//! A company's Microsoft 365 application, declared by its connector
//! administrators (ADR-116): directory and client IDs checked, the secret
//! sealed, never returned nor audited, one per company; the address to
//! register shown with how many people linked their account; its secret
//! replaced, never removed; its addresses the platform's.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::account_links::DIRECTORY;
use support::refusals::latest_entry;
use support::sso::{SSO_DEPLOYMENT, audit_text_since};
use support::{Client, TestApi, WEB_ORIGIN};

const DECLARE: &str = "/admin/connectors/microsoft365";
const CLIENT: &str = "0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0";
const SECRET: &str = "Secret~Application.2026_abc";

fn declaration() -> Value {
    json!({
        "name": "Microsoft 365",
        "directory": DIRECTORY,
        "clientId": CLIENT,
        "clientSecret": SECRET,
    })
}

async fn microsoft365_view(paul: &Client<'_>) -> Value {
    let listed = paul.get("/admin/connectors").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    let body = listed.json();
    let found = body["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "microsoft365")
        .cloned();
    found.unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_declares_its_application_once() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let types = paul.get("/admin/connectors").await.json()["types"].clone();
    assert!(
        types
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["type"] == "microsoft365"),
        "{types}"
    );

    for (field, value, said) in [
        ("directory", "contoso.onmicrosoft.com", "ID de locataire"),
        ("clientId", "mimorya", "ID client"),
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
    let view = microsoft365_view(&paul).await;
    assert_eq!(
        view["microsoft365"],
        json!({
            "directory": DIRECTORY,
            "clientId": CLIENT,
            "redirectUri": format!("{WEB_ORIGIN}/api/account/links/callback"),
            "secretStored": true,
            "links": 0,
        })
    );
    assert!(!view.to_string().contains(SECRET));
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(
        audited.contains(DIRECTORY) && audited.contains(CLIENT),
        "{audited}"
    );
    assert!(!audited.contains(SECRET), "{audited}");

    // One per company, whatever its name.
    let mut other = declaration();
    other["name"] = json!("Autre annuaire");
    let twice = paul.post(DECLARE, other).await;
    assert_eq!(twice.status, StatusCode::CONFLICT, "{}", twice.text());

    let removed = paul.delete(&format!("/admin/connectors/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(microsoft365_view(&paul).await, Value::Null);
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
            json!({ "url": "https://graph.example/v1.0" }),
            "ne changent pas",
        ),
        (
            json!({ "toolPolicies": { "m365_mail_send": { "enabled": false } } }),
            "Outil inconnu de ce connecteur",
        ),
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
            json!({ "token": "Nouveau~Secret.2027", "name": "Messagerie" }),
        )
        .await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.text());
    let view = microsoft365_view(&paul).await;
    assert_eq!(view["name"], json!("Messagerie"));
    assert_eq!(view["microsoft365"]["secretStored"], json!(true));
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(
        audited.contains("replaced") && !audited.contains("Nouveau~Secret"),
        "{audited}"
    );

    let removed = paul.delete(&path).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}
