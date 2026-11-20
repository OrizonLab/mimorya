//! The company's guard of the actions (ADR-213, D5) on the API, Jev the
//! engine. No action leaves without a person's validation on the platform
//! as it is, so what the guard does is told to whoever decides: active, the
//! signals it raises join the request's reason, and its decision is counted
//! in the run; observed, failed or off, the reason is the policy's own.
//! Only the tool, the action, its parameters and the person's domain are
//! sent. (The validation it adds to an action allowed alone is tested on
//! the engine: `crates/agent/tests/runtime_action_guard.rs`.)

mod support;

use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::{FakeJev, JEV_KEY, JevBehaviour};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::decisions::{set, settings};
use support::planning::{apply_proposal, propose};
use support::{Client, TestApi};

/// The guard of the actions set to `mode` at 0.5, Jev the engine.
fn guarding(mode: &str) -> Value {
    let mut body = settings("jev", Some(JEV_KEY));
    body["usages"][4] = json!({ "key": "actionGuard", "mode": mode, "threshold": 0.5 });
    body
}

/// A run that proposes a slot, then asks to apply it: its run and the
/// reason of the validation it awaits. `decided` is what the main model
/// answers when it is asked to decide.
async fn requested(
    api: &TestApi,
    marc: &Client<'_>,
    (intervention, start_time): (&str, &str),
    decided: Option<ScriptedStep>,
) -> (String, String) {
    let mut steps = vec![propose(intervention, start_time), apply_proposal("")];
    steps.extend(decided);
    api.script(steps);
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

/// The calls with the role « décision » counted for `run_id`.
async fn decisions_of(api: &TestApi, run_id: &str) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let count = sqlx::query_scalar(
        "select count(*) from usage_records where role = 'decision' and run_id = $1::uuid",
    )
    .bind(run_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    count
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn active_its_signals_are_told_to_whoever_decides() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;

    // Off: nothing asked, the policy's own reason.
    set(&paul, guarding("off")).await;
    let (_, policy_reason) = requested(&api, &marc, ("INT-2049", "09:00"), None).await;
    assert!(jev.asked().is_empty());

    // Active: Jev says yes to every question, the lowest risk.
    set(&paul, guarding("active")).await;
    let (run_id, reason) = requested(&api, &marc, ("INT-2052", "10:00"), None).await;
    assert_eq!(
        reason,
        format!(
            "{policy_reason} · Signalé par le service de décision : destinataire externe, \
             montant élevé, données personnelles, action irréversible"
        )
    );
    // Asked of the action alone (the proposal only drafts), on what it
    // declares.
    let asked = jev.asked();
    assert_eq!(asked.len(), 1);
    let state = asked[0].body["state"].as_object().unwrap();
    assert_eq!(
        state.keys().collect::<Vec<_>>(),
        ["outil", "action", "parametres", "domaine_interne"]
    );
    assert_eq!(state["domaine_interne"], "neo-maintenance.example");
    assert!(state["outil"].as_str().unwrap().contains("planning_apply"));
    // Its decision counted in the run.
    assert_eq!(decisions_of(&api, &run_id).await, 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn observed_or_failed_the_reason_is_the_policy_s_own() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    set(&paul, guarding("off")).await;
    let (_, policy_reason) = requested(&api, &marc, ("INT-2049", "11:00"), None).await;

    // Observed: asked and counted, without effect.
    set(&paul, guarding("observe")).await;
    let (run_id, reason) = requested(&api, &marc, ("INT-2052", "13:00"), None).await;
    assert_eq!(reason, policy_reason);
    assert_eq!(jev.asked().len(), 1);
    assert_eq!(decisions_of(&api, &run_id).await, 1);

    // Failed, Jev overloaded and the main model too: the request is the
    // policy's own, nothing added.
    set(&paul, guarding("active")).await;
    jev.behave(JevBehaviour::Overloaded);
    let failed = Some(ScriptedStep::fails("surchargé"));
    let (_, reason) = requested(&api, &marc, ("INT-2049", "14:00"), failed).await;
    assert_eq!(reason, policy_reason);
}
