//! The connectors an evaluation offers (ADR-153): a version the agent
//! proposed that no administrator made active, awaiting a decision or
//! rejected, is played, and the active version beside it, without the tools
//! of the connectors that reach outside the platform (an MCP server, a REST
//! API), each named to the agent as unavailable: a call it makes anyway is
//! refused before the API; the evaluation says so (ADR-156). A version an
//! administrator wrote, or a proposal once made active, is played with
//! them, as a run would be.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::{RecordedRequest, ScriptedStep, call, last_tool_results};
use mimorya_connectors::testing::{serve_demo_docs_http, serve_rest_http};
use mimorya_core::demo::user;
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::json;
use support::mcp::{PRIVATE_DEPLOYMENT, TOKEN, TOOL, add, mcp_body};
use support::rest::{KEY, READ, add_rest, rest_body};
use support::skill_evaluations::{
    detail, evaluations, forget, launched, new_version, play, played, verdict, write_case,
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

/// A Lucas proposal of the weekly report, validated: version 2.
async fn proposal(api: &TestApi) {
    let lucas = api.sign_in(user::LUCAS).await;
    let (_, approval) = proposed(api, &lucas, version_of_the_report(), read_proposed()).await;
    validate(api, &lucas, &approval).await;
}

fn user_turn(request: &RecordedRequest) -> String {
    match request.messages.first() {
        Some(LlmMessage::User { content }) => content.clone(),
        other => panic!("{other:?}"),
    }
}

/// The agent answers at once; the judge passes it.
fn answers() -> Vec<ScriptedStep> {
    played(false, "Trois retards cette semaine.", true)
}

/// The agent calls the REST API's tool, then answers; the judge passes it.
fn calls_the_api() -> Vec<ScriptedStep> {
    vec![
        ScriptedStep::calls("", vec![call(READ, json!({ "id": "INT-42" }))]),
        ScriptedStep::text("Le CRM ne répond pas."),
        verdict(true),
    ]
}

/// Evaluates `version` on one case, played as `script` has it for the
/// version and for the active one, and listed as played `inside_only` or
/// not (ADR-156); its id, and the requests of each version's agent and
/// judge, the version's first.
async fn evaluate(
    api: &TestApi,
    paul: &Client<'_>,
    version: i32,
    script: fn() -> Vec<ScriptedStep>,
    inside_only: bool,
) -> (String, [Vec<RecordedRequest>; 2]) {
    let mut both = script();
    let steps = both.len();
    both.extend(script());
    api.script(both);
    let model = api.model.as_ref().unwrap();
    let before = model.requests().len();
    let id = launched(paul, DEMO_SKILL_ID, version).await;
    play(api).await;
    let listed = evaluations(paul, DEMO_SKILL_ID).await;
    assert_eq!(listed[0]["status"], "completed", "{}", listed[0]);
    assert_eq!(listed[0]["referenceVersion"], 1);
    assert_eq!(listed[0]["insideOnly"], inside_only, "{}", listed[0]);
    let requests = model.requests().split_off(before);
    assert_eq!(requests.len(), 2 * steps);
    let (evaluated, active) = requests.split_at(steps);
    (id, [evaluated.to_vec(), active.to_vec()])
}

/// Neither tool offered, both connectors named as unavailable.
fn assert_withheld(requests: &[RecordedRequest]) {
    let tools = requests[0].tool_names();
    assert!(tools.contains(&"skill_read".to_string()), "{tools:?}");
    for withheld in [READ, TOOL] {
        assert!(!tools.contains(&withheld.to_string()), "{tools:?}");
    }
    let turn = user_turn(&requests[0]);
    assert!(turn.starts_with(NOTICE), "{turn}");
    assert!(turn.contains("« CRM »"), "{turn}");
    assert!(turn.contains("« Procédures »"), "{turn}");
}

/// Both tools offered, no connector named as unavailable.
fn assert_offered(requests: &[RecordedRequest]) {
    let tools = requests[0].tool_names();
    assert!(tools.contains(&READ.to_string()), "{tools:?}");
    assert!(tools.contains(&TOOL.to_string()), "{tools:?}");
    assert!(!user_turn(&requests[0]).contains(NOTICE));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_of_the_agent_is_played_without_what_reaches_outside() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let connectors = outside_connectors(&paul).await;
    proposal(&api).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;

    // The proposal (version 2), and the active version beside it: the
    // tool it calls anyway is refused before the API.
    let (id, played) = evaluate(&api, &paul, 2, calls_the_api, true).await;
    for requests in &played {
        assert_withheld(requests);
        let result = &last_tool_results(&requests[1])[0];
        assert!(result.is_error, "{}", result.content);
        assert!(
            result.content.contains("inconnu ou désactivé"),
            "{}",
            result.content
        );
    }
    // No tool ran for either version.
    let read = detail(&paul, &id).await.json();
    for version in ["result", "reference"] {
        assert_eq!(read["cases"][0][version]["tools"], json!([]), "{read}");
    }

    // Once made active by an administrator, it is the company's procedure.
    for version in [2, 1] {
        let activated = paul
            .put(
                &format!("{SKILLS}/{DEMO_SKILL_ID}/activation"),
                json!({ "version": version }),
            )
            .await;
        assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());
    }
    let (_, played) = evaluate(&api, &paul, 2, answers, false).await;
    played.iter().for_each(|requests| assert_offered(requests));
    remove(&paul, &connectors).await;
    forget(&api, DEMO_SKILL_ID).await;
    restore_the_report(&api).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_rejected_proposal_is_played_without_them_too() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let connectors = outside_connectors(&paul).await;
    proposal(&api).await;
    let rejected = paul
        .post(
            &format!("{SKILLS}/{DEMO_SKILL_ID}/rejection"),
            json!({ "version": 2 }),
        )
        .await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;

    let (_, played) = evaluate(&api, &paul, 2, answers, true).await;
    played.iter().for_each(|requests| assert_withheld(requests));
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

    let (_, played) = evaluate(&api, &paul, version, answers, false).await;
    played.iter().for_each(|requests| assert_offered(requests));
    remove(&paul, &connectors).await;
    forget(&api, DEMO_SKILL_ID).await;
    restore_the_report(&api).await;
}
