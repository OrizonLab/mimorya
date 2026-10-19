//! What administrators read of a connector's state (ADR-025): "untested"
//! until a start, then how the last start of the current configuration went.
//! A new address or token, or switching it back on, makes it untested again;
//! a report made on an older configuration is ignored, and a successful test
//! lets the next run try a remembered failure again. Servers written by the
//! operator keep their prefix and are not deleted from here.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::mcp::ACCESS_DENIED;
use mimorya_connectors::testing::{Quirk, serve_demo_docs_http, serve_mcp_http_on};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::connectors::{get_connector_instance, report_connector_status};
use serde_json::{Value, json};
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, TOOL, add, listed, mcp_body};
use uuid::Uuid;

async fn state(admin: &support::Client<'_>, id: &str) -> (String, Value) {
    let instance = listed(admin, id).await;
    (
        instance["status"].as_str().unwrap().to_string(),
        instance["lastError"].clone(),
    )
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_state_shows_only_what_was_observed() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let connectors = &api.app.services.connectors;
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let id = add(&paul, mcp_body("Procédures", "docs", &url, Some(TOKEN))).await;
    let instance = format!("/admin/connectors/{id}");
    assert_eq!(state(&paul, &id).await, ("untested".into(), Value::Null));
    connectors.tools_for(tenant::ID).await.unwrap();
    assert_eq!(state(&paul, &id).await.0, "ok");

    // A name or policies: what was observed still holds.
    for change in [
        json!({ "name": "Procédures internes" }),
        json!({ "toolPolicies": { TOOL: { "risk": "read" } } }),
    ] {
        assert_eq!(paul.patch(&instance, change).await.status, StatusCode::OK);
        assert_eq!(state(&paul, &id).await.0, "ok");
    }
    // Another token: untested, then refused by the server.
    let wrong = paul
        .patch(&instance, json!({ "token": "jeton-faux-00000000" }))
        .await;
    assert_eq!(wrong.status, StatusCode::OK);
    assert_eq!(state(&paul, &id).await.0, "untested");
    connectors.tools_for(tenant::ID).await.unwrap();
    let (status, error) = state(&paul, &id).await;
    assert_eq!(status, "error");
    assert!(
        error.as_str().unwrap().starts_with(ACCESS_DENIED),
        "{error}"
    );
    // The right token: the error of the previous one is gone.
    let fixed = paul.patch(&instance, json!({ "token": TOKEN })).await;
    assert_eq!(fixed.status, StatusCode::OK);
    assert_eq!(state(&paul, &id).await, ("untested".into(), Value::Null));
    let tested = paul.post(&format!("{instance}/test"), json!({})).await;
    assert_eq!(tested.json()["ok"], true);
    assert_eq!(state(&paul, &id).await.0, "ok");

    // Switched off, then on again.
    for (enabled, expected) in [(false, "disabled"), (true, "untested")] {
        let switched = paul.patch(&instance, json!({ "enabled": enabled })).await;
        assert_eq!(switched.status, StatusCode::OK);
        assert_eq!(state(&paul, &id).await.0, expected);
    }

    // A report made on the previous configuration is ignored.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id_value = Uuid::parse_str(&id).unwrap();
    let before = get_connector_instance(&mut tx, id_value)
        .await
        .unwrap()
        .unwrap();
    tx.commit().await.unwrap();
    let other = serve_demo_docs_http(None).await;
    let moved = paul.patch(&instance, json!({ "url": other })).await;
    assert_eq!(moved.status, StatusCode::OK);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    report_connector_status(
        &mut tx,
        id_value,
        before.updated_at,
        Some("ancienne adresse"),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(state(&paul, &id).await, ("untested".into(), Value::Null));
    assert_eq!(paul.delete(&instance).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn operator_servers_keep_their_prefix_and_stay() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let operated = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "insert into connector_instances (id, tenant_id, type, name, config) values ($1, $2, 'mcp', 'Interne', $3)",
    )
    .bind(operated)
    .bind(tenant::ID)
    .bind(json!({ "transport": "stdio", "prefix": "ops", "command": "mcp-interne" }))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let url = serve_demo_docs_http(None).await;
    let taken = paul
        .post("/admin/connectors", mcp_body("Autre", "ops", &url, None))
        .await;
    assert_eq!(taken.status, StatusCode::CONFLICT, "{}", taken.text());
    let instance = format!("/admin/connectors/{operated}");
    assert_eq!(paul.delete(&instance).await.status, StatusCode::BAD_REQUEST);
    let renamed = paul
        .patch(&instance, json!({ "name": "Outils internes" }))
        .await;
    assert_eq!(renamed.status, StatusCode::OK);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from connector_instances where id = $1")
        .bind(operated)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_successful_test_lets_the_next_run_try_again() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let connectors = &api.app.services.connectors;
    // Nothing listens on this port yet.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let id = add(&paul, mcp_body("Procédures", "docs", &url, None)).await;
    assert_eq!(
        connectors.tools_for(tenant::ID).await.unwrap().issues.len(),
        1
    );

    // The server comes back: runs keep the remembered failure for a while...
    serve_mcp_http_on(port, None, Quirk::None).await;
    assert_eq!(
        connectors.tools_for(tenant::ID).await.unwrap().issues.len(),
        1
    );
    // ...until a test succeeds.
    let tested = paul
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await;
    assert_eq!(tested.json()["ok"], true);
    let tools = connectors.tools_for(tenant::ID).await.unwrap();
    assert!(tools.issues.is_empty());
    assert!(tools.tools.iter().any(|t| t.info().name == TOOL));
    assert_eq!(state(&paul, &id).await.0, "ok");
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
