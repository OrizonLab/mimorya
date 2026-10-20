//! An MCP server is tried before it is saved (ADR-050): started with the
//! address and token typed in the form, its tools listed as they declare
//! themselves, then released. Nothing is stored; the audit keeps the address
//! and the outcome, never the token. The address is checked as when the
//! server is added: one this platform may not reach is refused before any
//! connection. Only an administrator of the whole company may try one.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::serve_demo_docs_http;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, TOOL};
use support::refusals::{entries_since, latest_entry, refusals_since};

const TRIAL: &str = "/admin/connectors/trial";

fn trial(url: &str, token: Option<&str>) -> Value {
    let mut body = json!({ "prefix": "docs", "url": url });
    if let Some(token) = token {
        body["token"] = json!(token);
    }
    body
}

async fn instances(admin: &support::Client<'_>) -> usize {
    admin.get("/admin/connectors").await.json()["instances"]
        .as_array()
        .unwrap()
        .len()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_server_is_tried_with_its_token_and_nothing_is_stored() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let before = instances(&paul).await;
    let since = latest_entry(&api).await;

    let tried = paul.post(TRIAL, trial(&url, Some(TOKEN))).await;
    assert_eq!(tried.status, StatusCode::OK, "{}", tried.text());
    let tried = tried.json();
    assert_eq!(tried["ok"], true, "{tried}");
    // As the server declares it: human validation and connector:use only.
    assert_eq!(tried["tools"][0]["name"], TOOL);
    assert_eq!(tried["tools"][0]["risk"], "write");
    assert_eq!(tried["tools"][0]["permissions"], json!(["connector:use"]));

    assert_eq!(instances(&paul).await, before);
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("connector.tried"), "{audited}");
    assert!(audited.contains(r#""tokenSent":true"#), "{audited}");
    assert!(audited.contains(&url), "{audited}");
    assert!(!audited.contains(TOKEN), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_failed_trial_says_why_without_the_token() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let since = latest_entry(&api).await;
    let wrong = "jeton-errone-0123456789";

    let tried = paul.post(TRIAL, trial(&url, Some(wrong))).await.json();
    assert_eq!(tried["ok"], false, "{tried}");
    assert_eq!(tried["tools"], json!([]));
    let error = tried["error"].as_str().unwrap();
    assert!(!error.is_empty());
    assert!(!error.contains(wrong), "{error}");
    let audited = entries_since(&api, since).await;
    assert!(audited.contains(r#""ok":false"#), "{audited}");
    assert!(!audited.contains(wrong), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_trial_is_checked_as_an_addition_and_reserved_to_administrators() {
    // The platform's default: public addresses over https only.
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let since = latest_entry(&api).await;

    for (body, problem) in [
        (trial("https://10.0.0.7/mcp", None), "réseau privé"),
        (trial("http://mcp.exemple.fr/mcp", None), "https"),
        (
            json!({ "prefix": "Docs!", "url": "https://mcp.exemple.fr/mcp" }),
            "Préfixe",
        ),
        (
            trial("https://mcp.exemple.fr/mcp", Some("jeton avec espaces")),
            "Jeton",
        ),
    ] {
        let refused = paul.post(TRIAL, body).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        let message = refused.text();
        assert!(message.contains(problem), "{problem}: {message}");
    }
    // A strict body: an unknown field is refused.
    let mut unknown = trial("https://mcp.exemple.fr/mcp", None);
    unknown["name"] = json!("Procédures");
    assert_eq!(
        paul.post(TRIAL, unknown).await.status,
        StatusCode::BAD_REQUEST
    );
    // Refused checks reach no server and leave no trial in the audit.
    assert!(!entries_since(&api, since).await.contains("connector.tried"));

    // A member is refused, and the refusal is audited (ADR-038).
    let tried = marc
        .post(TRIAL, trial("https://mcp.exemple.fr/mcp", None))
        .await;
    assert_eq!(tried.status, StatusCode::FORBIDDEN);
    assert_eq!(refusals_since(&api, since).await.len(), 1);
}
