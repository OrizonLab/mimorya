//! MCP connectors added by a company (ADR-025): over HTTPS at a public
//! address (or the private network when the operator allows it), with a
//! token sealed and never shown nor audited, which never follows the
//! connector to another address; per-tool policies scoped to the connector.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::serve_demo_docs_http;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::llm::MASTER_KEY;
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, TOOL, add, listed, mcp_body};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_mcp_server_is_added_tested_and_mapped() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let id = add(&paul, mcp_body("Procédures", "docs", &url, Some(TOKEN))).await;
    let shown = paul.get("/admin/connectors").await;
    assert!(!shown.text().contains(TOKEN));
    let instance = listed(&paul, &id).await;
    assert_eq!(instance["mcp"]["tokenStored"], true);
    assert_eq!(instance["mcp"]["url"], url);

    // Unmapped: human validation and connector:use only.
    let tested = paul
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await
        .json();
    assert_eq!(tested["ok"], true, "{tested}");
    assert_eq!(tested["tools"][0]["name"], TOOL);
    assert_eq!(tested["tools"][0]["risk"], "write");
    assert_eq!(tested["tools"][0]["permissions"], json!(["connector:use"]));

    let mapped = paul
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "toolPolicies": { TOOL: { "risk": "read", "permissions": ["intervention:read"] } } }),
        )
        .await;
    assert_eq!(mapped.status, StatusCode::OK, "{}", mapped.text());
    let tested = paul
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await
        .json();
    assert_eq!(tested["tools"][0]["risk"], "read");
    assert_eq!(
        tested["tools"][0]["permissions"],
        json!(["connector:use", "intervention:read"])
    );

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let sealed: Vec<u8> = sqlx::query_scalar("select ciphertext from secrets where name = $1")
        .bind(format!("connector.{id}.token"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(!sealed.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes()));
    let audited: Vec<Value> =
        sqlx::query_scalar("select metadata from audit_log where resource_id = $1")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert!(audited.len() >= 4);
    assert!(audited.iter().all(|m| !m.to_string().contains(TOKEN)));
    drop(tx);

    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
    assert_eq!(listed(&paul, &id).await, Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_reaches_public_https_servers_only() {
    // The default: a SaaS platform.
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let paul = api.sign_in(user::PAUL).await;
    for (url, prefix) in [
        ("http://mcp.example.com/mcp", "a"),
        ("https://127.0.0.1:8443/mcp", "b"),
        ("https://169.254.169.254/latest", "c"),
        ("https://[::1]/mcp", "d"),
        ("https://mcp.example.com/mcp?api_key=secret", "e"),
        ("https://user:pass@mcp.example.com/mcp", "f"),
    ] {
        let refused = paul
            .post(
                "/admin/connectors",
                mcp_body("Serveur", &format!("{prefix}x"), url, None),
            )
            .await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{url}: {}",
            refused.text()
        );
    }
    // A public name is accepted; reaching it is checked at each start.
    let id = add(
        &paul,
        mcp_body("Public", "pub", "https://mcp.example.com/mcp", None),
    )
    .await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_rules_and_rights_of_connector_changes() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_demo_docs_http(None).await;
    let id = add(&paul, mcp_body("Procédures", "docs", &url, Some(TOKEN))).await;
    let instance = format!("/admin/connectors/{id}");
    for (body, expected) in [
        (
            mcp_body("Autre", "Docs", &url, None),
            StatusCode::BAD_REQUEST,
        ),
        (mcp_body("Autre", "docs", &url, None), StatusCode::CONFLICT),
        (
            mcp_body("Procédures", "autre", &url, None),
            StatusCode::CONFLICT,
        ),
        (
            mcp_body("Autre", "autre", &url, Some("avec espaces 123")),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let refused = paul.post("/admin/connectors", body.clone()).await;
        assert_eq!(refused.status, expected, "{body}: {}", refused.text());
    }
    for change in [
        json!({ "toolPolicies": { "interventions_search": { "enabled": false } } }),
        json!({ "toolPolicies": { "mcp_autre_outil": { "enabled": false } } }),
        json!({ "toolPolicies": { TOOL: { "permissions": ["user:manage"] } } }),
        json!({ "toolPolicies": { TOOL: { "requiresApproval": false } } }),
        json!({ "url": "ftp://serveur" }),
        json!({ "prefix": "autre" }),
    ] {
        let refused = paul.patch(&instance, change.clone()).await;
        assert!(
            refused.status.is_client_error(),
            "{change}: {}",
            refused.text()
        );
    }

    // Another address: the token stays behind.
    let other = serve_demo_docs_http(None).await;
    assert_eq!(
        paul.patch(&instance, json!({ "url": other })).await.status,
        StatusCode::OK
    );
    assert_eq!(listed(&paul, &id).await["mcp"]["tokenStored"], false);
    assert_eq!(
        paul.patch(&instance, json!({ "token": TOKEN }))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        paul.patch(&instance, json!({ "token": null })).await.status,
        StatusCode::OK
    );
    assert_eq!(listed(&paul, &id).await["mcp"]["tokenStored"], false);

    // The demo connector is the operator's: only its name and activation change.
    let demo = listed_by_type(&paul, "demo_interventions").await;
    let demo_url = format!("/admin/connectors/{}", demo["id"].as_str().unwrap());
    assert_eq!(
        paul.patch(&demo_url, json!({ "url": url })).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(paul.delete(&demo_url).await.status, StatusCode::BAD_REQUEST);
    // Its test shows what runs require: its own permissions, not connector:use.
    let tested = paul
        .post(&format!("{demo_url}/test"), json!({}))
        .await
        .json();
    assert_eq!(tested["ok"], true, "{tested}");
    let tools = tested["tools"].as_array().unwrap();
    assert!(tools.iter().all(|t| {
        !t["permissions"]
            .as_array()
            .unwrap()
            .contains(&json!("connector:use"))
    }));
    assert!(tools.iter().any(|t| {
        t["permissions"]
            .as_array()
            .unwrap()
            .contains(&json!("intervention:read"))
    }));
    assert_eq!(
        paul.patch(&demo_url, json!({ "enabled": false }))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        listed_by_type(&paul, "demo_interventions").await["status"],
        "disabled"
    );
    assert_eq!(
        paul.patch(&demo_url, json!({ "enabled": true }))
            .await
            .status,
        StatusCode::OK
    );

    // Company-wide connector administrators only.
    let marc = api.sign_in(user::MARC).await;
    assert_eq!(
        marc.get("/admin/connectors").await.status,
        StatusCode::FORBIDDEN
    );
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.connecteurs").await;
    let partial = activate(&api, &mut partial).await;
    assert_eq!(
        partial
            .patch(&instance, json!({ "enabled": false }))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        partial
            .post(&format!("{instance}/test"), json!({}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(paul.delete(&instance).await.status, StatusCode::OK);
}

async fn listed_by_type(admin: &support::Client<'_>, kind: &str) -> Value {
    admin.get("/admin/connectors").await.json()["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == kind)
        .cloned()
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_master_key_no_token_is_kept() {
    let api = TestApi::start_with(&[("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true")]).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_demo_docs_http(None).await;
    let refused = paul
        .post(
            "/admin/connectors",
            mcp_body("Procédures", "docs", &url, Some(TOKEN)),
        )
        .await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    let id = add(&paul, mcp_body("Procédures", "docs", &url, None)).await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
