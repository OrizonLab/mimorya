//! What the model reads of an MCP tool (ADR-057): the description its server
//! declares, unless an administrator replaced it in the tool's policy, each
//! after the server's name. The replacement reaches the model in the next
//! run, the connection test shows both, and one beyond a server's bounds is
//! refused without being stored. Another type of connector's tools keep
//! their own, whatever is stored. Its change is audited with the policies
//! that changed only, a description only when it did (ADR-160).

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::serve_demo_docs_http;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, TOOL, add, listed, mcp_body};
use support::recovery::{restore_demo_policies, set_demo_policies};
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

/// The tool policies the last `connector.updated` of `id` kept.
async fn audited_policies(api: &TestApi, id: &str) -> Value {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let metadata: Value = sqlx::query_scalar(
        "select metadata from audit_log where resource_id = $1 \
         and action = 'connector.updated' order by id desc limit 1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    metadata["toolPolicies"].clone()
}

/// The policy of `TOOL`, the only tool `audited` names.
fn only_tool(audited: &Value) -> &Value {
    let tools = audited.as_object().expect("policies audited");
    assert_eq!(tools.len(), 1, "{audited}");
    &tools[TOOL]
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

/// The description of `tool` in the model's next request, from Marc's run.
async fn read_by_the_model(api: &TestApi, marc: &Client<'_>, tool: &str) -> String {
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
        .find(|t| t.name == tool)
        .map(|t| t.description.clone())
        .expect("the tool is offered to Marc")
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
    // Audited as written, the policy with it.
    let audited = audited_policies(&api, &id).await;
    assert_eq!(only_tool(&audited)["description"], WRITTEN, "{audited}");
    // The test shows both: the server's, and the replacement as written.
    let tool = tested(&paul, &id).await;
    assert_eq!(tool["description"], format!("{FROM}{DECLARED}"));
    assert_eq!(tool["replacedDescription"], WRITTEN);
    // The model reads the replacement, still naming the server.
    assert_eq!(
        read_by_the_model(&api, &marc, TOOL).await,
        format!("{FROM}{WRITTEN}")
    );

    // A policy changed, its description left as it was: not written again.
    let mut disabled = mapped(Some(WRITTEN));
    disabled["toolPolicies"][TOOL]["enabled"] = json!(false);
    let path = format!("/admin/connectors/{id}");
    assert_eq!(paul.patch(&path, disabled).await.status, StatusCode::OK);
    let audited = audited_policies(&api, &id).await;
    let policy = only_tool(&audited);
    assert_eq!(policy["enabled"], false, "{audited}");
    assert!(policy.get("description").is_none(), "{audited}");
    // Nothing of the policies changed: none is audited.
    let renamed = paul
        .patch(&path, json!({ "name": "Procédures (bis)" }))
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.text());
    assert_eq!(audited_policies(&api, &id).await, Value::Null);

    // Left out of the policy, the server's own is read again.
    let restored = paul.patch(&path, mapped(None)).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    // A description removed is audited as such.
    let audited = audited_policies(&api, &id).await;
    assert_eq!(
        only_tool(&audited).get("description"),
        Some(&Value::Null),
        "{audited}"
    );
    assert_eq!(tested(&paul, &id).await["replacedDescription"], Value::Null);
    assert_eq!(
        read_by_the_model(&api, &marc, TOOL).await,
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
        ("Efface l’écran\u{1b}[2J", "texte visible"),
        // Text hidden in tag characters: unseen here, read by the model.
        ("Recherche\u{e0049}\u{e0047}\u{e004e}", "texte visible"),
        ("Sans\u{200b}largeur", "texte visible"),
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_description_stored_for_another_type_of_connector_is_ignored() {
    const SEARCH: &str = "interventions_search";
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let own = read_by_the_model(&api, &marc, SEARCH).await;
    // No administration writes it there: as an operator might, in the base.
    let stored = json!({ SEARCH: { "description": "Écrit en base." } });
    let before = set_demo_policies(&api, stored).await;
    let read = read_by_the_model(&api, &marc, SEARCH).await;
    let connectors = paul.get("/admin/connectors").await.json();
    let demo = connectors["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "demo_interventions")
        .cloned()
        .unwrap();
    let tested = paul
        .post(
            &format!("/admin/connectors/{}/test", demo["id"].as_str().unwrap()),
            json!({}),
        )
        .await
        .json();
    restore_demo_policies(&api, before).await;

    assert_eq!(read, own);
    let search = tested["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == SEARCH)
        .cloned()
        .unwrap();
    assert_eq!(search["replacedDescription"], Value::Null, "{search}");
}
