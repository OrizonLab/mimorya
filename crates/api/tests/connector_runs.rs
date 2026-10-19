//! MCP connectors in runs (ADR-025): a mapped tool runs under its policy; a
//! server that fails is reported on its connector without affecting the
//! others; a connector's policies only reach its own tools, and two
//! connectors naming the same tool do not break the company's runs.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_connectors::testing::serve_demo_docs_http;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::json;
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, TOOL, add, listed, mcp_body};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mapped_mcp_tool_runs_under_its_policy() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let id = add(&paul, mcp_body("Procédures", "docs", &url, Some(TOKEN))).await;
    let mapped = paul
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "toolPolicies": { TOOL: { "risk": "read", "permissions": ["intervention:read"] } } }),
        )
        .await;
    assert_eq!(mapped.status, StatusCode::OK);

    api.script(vec![
        ScriptedStep::calls("", vec![call(TOOL, json!({ "query": "circulateur" }))]),
        ScriptedStep::text("Voici la procédure du circulateur."),
    ]);
    let conversation = marc.post("/conversations", json!({})).await.json();
    let conversation_id = conversation["conversation"]["id"].as_str().unwrap();
    let started = marc
        .post(
            &format!("/conversations/{conversation_id}/messages"),
            json!({ "content": "Quelle est la procédure du circulateur ?" }),
        )
        .await;
    let run_id = started.json()["runId"].as_str().unwrap().to_string();
    api.idle().await;
    let detail = marc.get(&format!("/runs/{run_id}")).await.json();
    let tool = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["toolName"] == TOOL)
        .cloned()
        .unwrap();
    // A read under intervention:read: no validation needed.
    assert_eq!(tool["status"], "succeeded", "{detail}");
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_failing_server_is_reported_without_affecting_the_others() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    // Nothing listens there.
    let id = add(
        &paul,
        mcp_body("Hors ligne", "off", "http://127.0.0.1:9/mcp", None),
    )
    .await;
    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    assert_eq!(tools.issues.len(), 1);
    assert!(
        tools
            .tools
            .iter()
            .any(|t| t.info().name == "interventions_search")
    );
    let instance = listed(&paul, &id).await;
    assert_eq!(instance["status"], "error");
    assert!(!instance["lastError"].as_str().unwrap().is_empty());
    let tested = paul
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await
        .json();
    assert_eq!(tested["ok"], false);

    // Back online at another address: the next start clears the error.
    let url = serve_demo_docs_http(None).await;
    assert_eq!(
        paul.patch(&format!("/admin/connectors/{id}"), json!({ "url": url }))
            .await
            .status,
        StatusCode::OK
    );
    api.app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    assert_eq!(listed(&paul, &id).await["status"], "ok");
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn policies_reach_their_own_tools_and_duplicates_stay_apart() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_demo_docs_http(None).await;
    let id = add(&paul, mcp_body("Procédures", "docs", &url, None)).await;
    // Written outside the API: a policy aimed at another connector's tool,
    // and permissions without connector:use; then a second connector with the
    // same prefix.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update connector_instances set tool_policies = $2, updated_at = now() where id = $1",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .bind(json!({
        "interventions_search": { "enabled": false },
        TOOL: { "permissions": ["intervention:read"] },
    }))
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "insert into connector_instances (tenant_id, type, name, config) values ($1, 'mcp', 'Copie', $2)",
    )
    .bind(tenant::ID)
    .bind(json!({ "transport": "http", "prefix": "docs", "url": url }))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    assert!(!tools.overrides.contains_key("interventions_search"));
    assert_eq!(
        tools.overrides[TOOL].permissions,
        Some(vec![
            "intervention:read".to_string(),
            "connector:use".to_string()
        ])
    );
    let named: Vec<&str> = tools
        .tools
        .iter()
        .map(|t| t.info().name.as_str())
        .filter(|n| *n == TOOL)
        .collect();
    assert_eq!(named.len(), 1);
    assert_eq!(tools.issues.len(), 1);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from connector_instances where type = 'mcp'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_companys_connector_does_not_exist_from_here() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let foreign = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query(
        "insert into connector_instances (id, tenant_id, type, name, config) values ($1, $2, 'mcp', 'Étranger', $3)",
    )
    .bind(foreign)
    .bind(OTHER_TENANT_ID)
    .bind(json!({ "transport": "http", "prefix": "ext", "url": "https://mcp.example.com/mcp" }))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let url = format!("/admin/connectors/{foreign}");
    assert_eq!(
        paul.patch(&url, json!({ "enabled": false })).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        paul.post(&format!("{url}/test"), json!({})).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(paul.delete(&url).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        listed(&paul, &foreign.to_string()).await,
        serde_json::Value::Null
    );
}
