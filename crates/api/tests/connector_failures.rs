//! One MCP connector failing never breaks a company's runs (ADR-025): tools
//! sharing an identifier, unreadable policies, a token quoted back by the
//! server, a server that never answers. Each one sets its own connector
//! aside, says why to its administrators, and never shows the token.

mod support;

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_agent::tools::ToolRegistry;
use mimorya_connectors::mcp::TOO_SLOW;
use mimorya_connectors::testing::{Quirk, serve_demo_docs_http, serve_mcp_http, serve_mute};
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::TestApi;
use support::llm::ask;
use support::mcp::{
    PRIVATE_DEPLOYMENT, TOKEN, TOOL, add, capture_logs, listed, mcp_body, write_policies,
};

async fn test_of(admin: &support::Client<'_>, id: &str) -> serde_json::Value {
    admin
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await
        .json()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_failing_connector_never_breaks_the_company_runs() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let colliding = serve_mcp_http(None, Quirk::CollidingTools).await;
    let doubled = add(&paul, mcp_body("Doublons", "dup", &colliding, None)).await;
    let docs = serve_demo_docs_http(None).await;
    let unreadable = add(&paul, mcp_body("Procédures", "docs", &docs, None)).await;
    // Written in the database (an operator's server, say): a risk that does not exist.
    write_policies(&api, &unreadable, json!({ TOOL: { "risk": "lecture" } })).await;

    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    assert_eq!(tools.issues.len(), 2);
    assert!(ToolRegistry::new(tools.tools.clone(), &tools.overrides).is_ok());
    assert!(
        tools
            .tools
            .iter()
            .any(|t| t.info().name == "interventions_search")
    );
    let set_aside = listed(&paul, &doubled).await;
    assert_eq!(set_aside["status"], "error");
    assert!(
        set_aside["lastError"]
            .as_str()
            .unwrap()
            .contains("même identifiant (mcp_dup_get_item)"),
        "{set_aside}"
    );
    let set_aside = listed(&paul, &unreadable).await;
    assert_eq!(set_aside["status"], "error");
    assert!(
        set_aside["lastError"]
            .as_str()
            .unwrap()
            .starts_with("Politiques d’outils illisibles"),
        "{set_aside}"
    );
    // The test says so too, rather than "connected".
    let tested = test_of(&paul, &doubled).await;
    assert_eq!(tested["ok"], false);
    assert!(
        tested["error"]
            .as_str()
            .unwrap()
            .contains("même identifiant")
    );
    let tested = test_of(&paul, &unreadable).await;
    assert_eq!(tested["ok"], false);
    assert!(tested["error"].as_str().unwrap().starts_with("Politiques"));

    // A conversation still runs, with the other connectors.
    api.script(vec![ScriptedStep::text("Bonjour Marc.")]);
    let marc = api.sign_in(user::MARC).await;
    let (status, answer) = ask(&api, &marc, "Bonjour").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(answer, "Bonjour Marc.");
    for id in [doubled, unreadable] {
        assert_eq!(
            paul.delete(&format!("/admin/connectors/{id}")).await.status,
            StatusCode::OK
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_token_quoted_back_reaches_neither_logs_nor_administrators_nor_the_model() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let at_start = serve_mcp_http(Some(TOKEN), Quirk::EchoesTokenOnStart).await;
    let id = add(&paul, mcp_body("Bavard", "echo", &at_start, Some(TOKEN))).await;
    let logs = capture_logs();
    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    let logged = logs.text();
    drop(logs);
    assert_eq!(tools.issues.len(), 1);
    assert!(logged.contains("Connector unavailable"), "{logged}");
    assert!(logged.contains("Bearer ***"), "{logged}");
    assert!(!logged.contains(TOKEN), "{logged}");
    let instance = listed(&paul, &id).await;
    assert!(
        instance["lastError"]
            .as_str()
            .unwrap()
            .contains("Bearer ***")
    );
    assert!(!instance.to_string().contains(TOKEN));
    assert!(!test_of(&paul, &id).await.to_string().contains(TOKEN));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // Quoted in the declarations: neither the test nor the model sees it.
    let declaring = serve_mcp_http(Some(TOKEN), Quirk::EchoesTokenInDeclarations).await;
    let id = add(&paul, mcp_body("Bavard", "echo", &declaring, Some(TOKEN))).await;
    let tested = test_of(&paul, &id).await;
    assert_eq!(tested["ok"], true, "{tested}");
    assert!(!tested.to_string().contains(TOKEN), "{tested}");
    assert!(tested.to_string().contains("Bearer ***"), "{tested}");
    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    for tool in tools
        .tools
        .iter()
        .filter(|t| t.info().name.starts_with("mcp_echo_"))
    {
        let declared = format!("{:?} {}", tool.info(), tool.input_schema());
        assert!(!declared.contains(TOKEN), "{declared}");
    }
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // Quoted back by a tool call: what the run keeps and the model reads.
    let on_call = serve_mcp_http(Some(TOKEN), Quirk::EchoesTokenOnCall).await;
    let id = add(&paul, mcp_body("Bavard", "echo", &on_call, Some(TOKEN))).await;
    let tool = "mcp_echo_search_procedures";
    let mapped = paul
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "toolPolicies": { tool: { "risk": "read", "permissions": ["intervention:read"] } } }),
        )
        .await;
    assert_eq!(mapped.status, StatusCode::OK);
    api.script(vec![
        ScriptedStep::calls("", vec![call(tool, json!({ "query": "circulateur" }))]),
        ScriptedStep::text("Le serveur a refusé."),
    ]);
    let marc = api.sign_in(user::MARC).await;
    let logs = capture_logs();
    let (_, answer) = ask(&api, &marc, "La procédure du circulateur ?").await;
    let logged = logs.text();
    drop(logs);
    assert_eq!(answer, "Le serveur a refusé.");
    assert!(!logged.contains(TOKEN), "{logged}");
    let model = api.model.as_ref().unwrap().requests();
    let seen = format!("{:?}", model.last().unwrap().messages);
    assert!(seen.contains("Bearer ***"), "{seen}");
    assert!(!seen.contains(TOKEN));
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let kept: i64 = sqlx::query_scalar(
        "select (select count(*) from tool_invocations where concat(output_summary, error, input::text) like $1) \
         + (select count(*) from run_events where data::text like $1) \
         + (select count(*) from messages where content like $1 or metadata::text like $1)",
    )
    .bind(format!("%{TOKEN}%"))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    drop(tx);
    assert_eq!(kept, 0);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mute_server_delays_one_run_per_retry_delay() {
    let mut deployment = PRIVATE_DEPLOYMENT.to_vec();
    deployment.push(("MIMORYA_MCP_START_TIMEOUT_SECS", "1"));
    let api = TestApi::start_with(&deployment).await;
    let paul = api.sign_in(user::PAUL).await;
    let first = add(&paul, mcp_body("Muet", "muet", &serve_mute().await, None)).await;
    let second = add(
        &paul,
        mcp_body("Muette", "muette", &serve_mute().await, None),
    )
    .await;
    let connectors = &api.app.services.connectors;

    // Side by side: one start timeout, not one per server.
    let started = Instant::now();
    let tools = connectors.tools_for(tenant::ID).await.unwrap();
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(1), "{waited:?}");
    assert!(waited < Duration::from_millis(1900), "{waited:?}");
    assert_eq!(tools.issues.len(), 2);
    assert!(
        tools
            .tools
            .iter()
            .any(|t| t.info().name == "interventions_search")
    );
    let instance = listed(&paul, &first).await;
    assert_eq!(instance["status"], "error");
    assert!(
        instance["lastError"]
            .as_str()
            .unwrap()
            .starts_with(TOO_SLOW),
        "{instance}"
    );

    // Remembered: the next runs do not wait for it.
    let started = Instant::now();
    let tools = connectors.tools_for(tenant::ID).await.unwrap();
    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(tools.issues.len(), 2);

    // At a new address, it is tried at once.
    let url = serve_demo_docs_http(None).await;
    let moved = paul
        .patch(&format!("/admin/connectors/{first}"), json!({ "url": url }))
        .await;
    assert_eq!(moved.status, StatusCode::OK);
    let tools = connectors.tools_for(tenant::ID).await.unwrap();
    assert_eq!(tools.issues.len(), 1);
    assert_eq!(listed(&paul, &first).await["status"], "ok");
    for id in [first, second] {
        assert_eq!(
            paul.delete(&format!("/admin/connectors/{id}")).await.status,
            StatusCode::OK
        );
    }
}
