//! A run that starts while connectors of the company could not start tells
//! its model and its user which ones (ADR-048), naming only those whose
//! tools the user could reach: an MCP connector to a holder of
//! `connector:use`, the demo connector to a holder of one of its business
//! permissions. Every connector up, nothing is told.

mod support;

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::serve_demo_docs_http;
use mimorya_core::demo::user;
use mimorya_db::seed::DEMO_CONNECTOR_ID;
use serde_json::{Value, json};
use support::mcp::{PRIVATE_DEPLOYMENT, TOOL, add, mcp_body, write_policies};
use support::runs::start_run;
use support::{Client, TestApi};

const DEMO: &str = "Interventions (démonstration)";
const NOTE: &str = "<connecteurs_indisponibles>";

/// A run of `client`, once over: the user turn its model received, and the
/// connectors its stream announced as unavailable.
async fn run_of(api: &TestApi, client: &Client<'_>) -> (String, Option<Value>) {
    api.script(vec![ScriptedStep::text("C’est noté.")]);
    let run_id = start_run(client).await;
    api.idle().await;
    let requests = api.model.as_ref().unwrap().requests();
    let Some(LlmMessage::User { content }) = requests.last().and_then(|r| r.messages.last()) else {
        panic!("a user turn last");
    };
    let stream = client.get(&format!("/runs/{run_id}/events")).await.text();
    let announced = stream
        .split("\n\n")
        .find(|event| event.contains("event: connectors_unavailable"))
        .and_then(|event| event.lines().find_map(|l| l.strip_prefix("data: ")))
        .map(|data| serde_json::from_str(data).unwrap());
    (content.clone(), announced)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_is_told_of_the_connectors_in_reach_that_could_not_start() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;

    // Every connector up: nothing is told.
    let (turn, announced) = run_of(&api, &marc).await;
    assert!(!turn.contains(NOTE), "{turn}");
    assert_eq!(announced, None);

    // An MCP server and the demo connector set aside (policies that cannot
    // be read: an operator's change, say).
    let docs = serve_demo_docs_http(None).await;
    let mcp = add(&paul, mcp_body("Procédures", "docs", &docs, None)).await;
    write_policies(&api, &mcp, json!({ TOOL: { "risk": "lecture" } })).await;
    let demo = DEMO_CONNECTOR_ID.to_string();
    write_policies(
        &api,
        &demo,
        json!({ "interventions_search": { "risk": "x" } }),
    )
    .await;

    // The technical manager could use both: both are named, oldest first.
    let (turn, announced) = run_of(&api, &marc).await;
    assert!(turn.starts_with(NOTE), "{turn}");
    assert!(
        turn.contains(&format!("« {DEMO} », « Procédures »")),
        "{turn}"
    );
    assert!(turn.ends_with("Fais le point sur la semaine"), "{turn}");
    assert_eq!(
        announced,
        Some(json!({ "connectors": [DEMO, "Procédures"] }))
    );

    // The technical administrator holds no permission of the demo
    // connector: only the MCP server is named to the runs of this account.
    let (turn, announced) = run_of(&api, &paul).await;
    assert!(turn.contains("« Procédures »"), "{turn}");
    assert!(!turn.contains(DEMO), "{turn}");
    assert_eq!(announced, Some(json!({ "connectors": ["Procédures"] })));

    write_policies(&api, &demo, json!({})).await;
}
