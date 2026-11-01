//! The connectors an evaluation offers (ADR-153): a version the agent
//! proposed that no administrator made active is played, and the active
//! version beside it, without the tools of the connectors that reach outside
//! the platform (an MCP server, a REST API), each named to the agent as
//! unavailable; a version an administrator wrote, or a proposal once made
//! active, is played with them, as a run would be.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::RecordedRequest;
use mimorya_connectors::testing::{serve_demo_docs_http, serve_rest_http};
use mimorya_core::demo::user;
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::json;
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, TOOL, add, mcp_body};
use support::rest::{KEY, READ, add_rest, rest_body};
use support::skill_evaluations::{
    evaluations, forget, launched, new_version, play, played, write_case,
};
use support::skill_proposals::{
    SKILLS, proposed, read_proposed, restore_the_report, validate, version_of_the_report,
};
use support::{Client, TestApi};

const LATE: &str = "Quels retards cette semaine ?";
const NOTICE: &str = "<connecteurs_indisponibles>";

/// The company's REST API and MCP server, each tool read without
/// validation; their ids.
async fn outside_connectors(paul: &Client<'_>) -> [String; 2] {
    let rest = add_rest(paul, rest_body("CRM", &serve_rest_http().await, Some(KEY))).await;
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let mcp = add(paul, mcp_body("Procédures", "docs", &url, Some(TOKEN))).await;
    for (id, tool) in [(&rest, READ), (&mcp, TOOL)] {
        let mapped = paul
            .patch(
                &format!("/admin/connectors/{id}"),
                json!({ "toolPolicies": { tool: { "risk": "read" } } }),
            )
            .await;
        assert_eq!(mapped.status, StatusCode::OK, "{}", mapped.text());
    }
    [rest, mcp]
}

async fn remove(paul: &Client<'_>, connectors: &[String]) {
    for id in connectors {
        let removed = paul.delete(&format!("/admin/connectors/{id}")).await;
        assert_eq!(removed.status, StatusCode::OK);
    }
}

fn user_turn(request: &RecordedRequest) -> String {
    match request.messages.first() {
        Some(LlmMessage::User { content }) => content.clone(),
        other => panic!("{other:?}"),
    }
}

/// Evaluates `version` on one case, played for it and for the active
/// version; the agent's requests, each version's first.
async fn evaluate(api: &TestApi, paul: &Client<'_>, version: i32) -> [RecordedRequest; 2] {
    let mut script = played(false, "Trois retards cette semaine.", true);
    script.extend(played(false, "Trois retards, par technicien.", true));
    api.script(script);
    let model = api.model.as_ref().unwrap();
    let before = model.requests().len();
    launched(paul, DEMO_SKILL_ID, version).await;
    play(api).await;
    let listed = evaluations(paul, DEMO_SKILL_ID).await;
    assert_eq!(listed[0]["status"], "completed", "{}", listed[0]);
    assert_eq!(listed[0]["referenceVersion"], 1);
    let requests = model.requests();
    // The agent then the judge, for each version.
    assert_eq!(requests.len(), before + 4);
    [requests[before].clone(), requests[before + 2].clone()]
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_of_the_agent_is_played_without_what_reaches_outside() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let connectors = outside_connectors(&paul).await;
    let (_, approval) = proposed(&api, &lucas, version_of_the_report(), read_proposed()).await;
    validate(&api, &lucas, &approval).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;

    // The proposal (version 2), and the active version beside it.
    for request in evaluate(&api, &paul, 2).await {
        let tools = request.tool_names();
        assert!(tools.contains(&"skill_read".to_string()), "{tools:?}");
        for withheld in [READ, TOOL] {
            assert!(!tools.contains(&withheld.to_string()), "{tools:?}");
        }
        let turn = user_turn(&request);
        assert!(turn.starts_with(NOTICE), "{turn}");
        assert!(turn.contains("« CRM »"), "{turn}");
        assert!(turn.contains("« Procédures »"), "{turn}");
    }

    // Once made active by an administrator, it is the company's procedure.
    let activated = paul
        .put(
            &format!("{SKILLS}/{DEMO_SKILL_ID}/activation"),
            json!({ "version": 2 }),
        )
        .await;
    assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());
    let reactivated = paul
        .put(
            &format!("{SKILLS}/{DEMO_SKILL_ID}/activation"),
            json!({ "version": 1 }),
        )
        .await;
    assert_eq!(reactivated.status, StatusCode::OK);
    for request in evaluate(&api, &paul, 2).await {
        let tools = request.tool_names();
        assert!(tools.contains(&READ.to_string()), "{tools:?}");
        assert!(tools.contains(&TOOL.to_string()), "{tools:?}");
        assert!(!user_turn(&request).contains(NOTICE));
    }
    remove(&paul, &connectors).await;
    forget(&api, DEMO_SKILL_ID).await;
    restore_the_report(&api).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_version_an_administrator_wrote_is_played_with_them() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let connectors = outside_connectors(&paul).await;
    let version = new_version(&paul).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;

    for request in evaluate(&api, &paul, version).await {
        let tools = request.tool_names();
        assert!(tools.contains(&READ.to_string()), "{tools:?}");
        assert!(tools.contains(&TOOL.to_string()), "{tools:?}");
        assert!(!user_turn(&request).contains(NOTICE));
    }
    remove(&paul, &connectors).await;
    forget(&api, DEMO_SKILL_ID).await;
    restore_the_report(&api).await;
}
