//! The company's detection of instructions meant for an AI in external
//! content (ADR-213, D6) on the API, Jev the engine: active, a result said
//! to hold some is marked for the person and the run, suspect, says so in
//! each later validation; observed or off, nothing changes. Only the tool
//! and the start of the content are sent. (The validation it adds to an
//! action allowed alone is tested on the engine:
//! `crates/agent/tests/runtime_content_screen.rs`.)

mod support;

use mimorya_connectors::testing::{FakeJev, JEV_KEY};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::decisions::{set, settings};
use support::planning::{apply_proposal, propose};
use support::{Client, TestApi};

/// The detection set to `mode` at 0.5, Jev the engine.
fn detecting(mode: &str) -> Value {
    let mut body = settings("jev", Some(JEV_KEY));
    body["usages"][5] = json!({ "key": "injectionDetection", "mode": mode, "threshold": 0.5 });
    body
}

/// A run that proposes a slot, then asks to apply it: its run and the
/// reason of the validation it awaits.
async fn requested(
    api: &TestApi,
    marc: &Client<'_>,
    (intervention, start_time): (&str, &str),
) -> (String, String) {
    api.script(vec![propose(intervention, start_time), apply_proposal("")]);
    let conversation = marc.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let started = marc
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": format!("Planifie {intervention}") }),
        )
        .await
        .json();
    let run_id = started["runId"].as_str().unwrap().to_string();
    api.idle().await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0, "steps left");
    let approvals = marc.get("/approvals").await.json();
    let approval = approvals["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["runId"] == run_id.as_str())
        .expect("validation requested")
        .clone();
    (run_id, approval["reason"].as_str().unwrap().to_string())
}

/// What the person reads of the proposal's result.
async fn proposal_summary(marc: &Client<'_>, run_id: &str) -> String {
    let detail = marc.get(&format!("/runs/{run_id}")).await.json();
    let tools = detail["tools"].as_array().unwrap();
    let proposal = tools
        .iter()
        .find(|t| t["toolName"] == "planning_propose")
        .expect("proposal recorded");
    proposal["outputSummary"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn active_a_suspect_result_is_marked_and_each_later_validation_says_so() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;

    // Off: nothing asked, the policy's own reason.
    set(&paul, detecting("off")).await;
    let (off_run, policy_reason) = requested(&api, &marc, ("INT-2049", "09:00")).await;
    assert!(jev.asked().is_empty());
    let clear = proposal_summary(&marc, &off_run).await;
    assert!(!clear.contains("instructions"), "{clear}");

    // Observed: asked, without effect.
    set(&paul, detecting("observe")).await;
    let (observed, reason) = requested(&api, &marc, ("INT-2052", "10:00")).await;
    assert_eq!(reason, policy_reason);
    let unmarked = proposal_summary(&marc, &observed).await;
    assert!(!unmarked.contains("instructions"), "{unmarked}");
    assert_eq!(jev.asked().len(), 1);

    // Active: Jev says yes; the proposal's result is marked and the run's
    // validation says why.
    set(&paul, detecting("active")).await;
    let (run_id, reason) = requested(&api, &marc, ("INT-2049", "11:00")).await;
    assert_eq!(
        reason,
        format!("{policy_reason} · Contenu externe suspect lu par ce run")
    );
    let marked = proposal_summary(&marc, &run_id).await;
    assert!(
        marked.contains("instructions destinées à l’IA détectées"),
        "{marked}"
    );
    // The tool and the start of its result alone were sent.
    let asked = jev.asked();
    let state = asked.last().unwrap().body["state"]
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(state.keys().collect::<Vec<_>>(), ["outil", "contenu"]);
    assert_eq!(state["outil"], "planning_propose");
}
