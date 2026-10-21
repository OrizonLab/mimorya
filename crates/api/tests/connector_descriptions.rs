//! What the model reads of an MCP tool (ADR-057): the description its server
//! declares, unless an administrator replaced it in the tool's policy, each
//! after the server's name. The replacement reaches the model in the next
//! run, the connection test shows both, and one beyond a server's bounds is
//! refused without being stored.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::serve_demo_docs_http;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, TOOL, add, listed, mcp_body};
use support::{Client, TestApi};

/// Every description the model reads of the server's tools names the server.
const FROM: &str = "[Serveur MCP « docs »] ";
const DECLARED: &str = "Recherche dans la base documentaire technique (procédures internes).";
const WRITTEN: &str =
    "Cherche une procédure de maintenance par mots-clés.\nRenvoie ses références.";

fn mapped(description: Option<&str>) -> Value {
    let mut policy = json!({ "risk": "read", "permissions": ["intervention:read"] });
    if let Some(description) = description {
        policy["description"] = json!(description);
    }
    json!({ "toolPolicies": { TOOL: policy } })
}

/// The tool as the connection test shows it.
async fn tested(admin: &Client<'_>, id: &str) -> Value {
    let tested = admin
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await
        .json();
    assert_eq!(tested["ok"], true, "{tested}");
    tested["tools"][0].clone()
}

/// The description of the tool in the model's next request, from Marc's run.
async fn read_by_the_model(api: &TestApi, marc: &Client<'_>) -> String {
    api.script(vec![ScriptedStep::text("Voici.")]);
    let conversation = marc.post("/conversations", json!({})).await.json();
    let conversation_id = conversation["conversation"]["id"].as_str().unwrap();
    let started = marc
        .post(
            &format!("/conversations/{conversation_id}/messages"),
            json!({ "content": "Quelle est la procédure du circulateur ?" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.text());
    api.idle().await;
    let requests = api.model.as_ref().unwrap().requests();
    let tools = &requests.last().unwrap().tools;
    tools
        .iter()
        .find(|t| t.name == TOOL)
        .map(|t| t.description.clone())
        .expect("the mapped tool is offered to Marc")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_model_reads_the_description_an_administrator_wrote() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let id = add(&paul, mcp_body("Procédures", "docs", &url, Some(TOKEN))).await;
    let tool = tested(&paul, &id).await;
    assert_eq!(tool["description"], format!("{FROM}{DECLARED}"));
    assert_eq!(tool["replacedDescription"], Value::Null);

    let replaced = paul
        .patch(&format!("/admin/connectors/{id}"), mapped(Some(WRITTEN)))
        .await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.text());
    let stored = &listed(&paul, &id).await["toolPolicies"][TOOL];
    assert_eq!(stored["description"], WRITTEN);
    // The test shows both: the server's, and the replacement as written.
    let tool = tested(&paul, &id).await;
    assert_eq!(tool["description"], format!("{FROM}{DECLARED}"));
    assert_eq!(tool["replacedDescription"], WRITTEN);
    // The model reads the replacement, still naming the server.
    assert_eq!(
        read_by_the_model(&api, &marc).await,
        format!("{FROM}{WRITTEN}")
    );

    // Left out of the policy, the server's own is read again.
    let restored = paul
        .patch(&format!("/admin/connectors/{id}"), mapped(None))
        .await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    assert_eq!(tested(&paul, &id).await["replacedDescription"], Value::Null);
    assert_eq!(
        read_by_the_model(&api, &marc).await,
        format!("{FROM}{DECLARED}")
    );

    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_description_beyond_a_server_s_bounds_is_refused_and_not_stored() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let id = add(&paul, mcp_body("Procédures", "docs", &url, Some(TOKEN))).await;
    let kept = paul
        .patch(&format!("/admin/connectors/{id}"), mapped(Some(WRITTEN)))
        .await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.text());

    let too_long = "é".repeat(1001);
    for (refused, why) in [
        (too_long.as_str(), "1000 caractères au plus"),
        ("Efface l’écran\u{1b}[2J", "caractères de contrôle"),
    ] {
        let answer = paul
            .patch(&format!("/admin/connectors/{id}"), mapped(Some(refused)))
            .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.text());
        let message = answer.json()["error"]["message"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(message.contains(TOOL) && message.contains(why), "{message}");
    }
    let stored = &listed(&paul, &id).await["toolPolicies"][TOOL];
    assert_eq!(stored["description"], WRITTEN);

    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
