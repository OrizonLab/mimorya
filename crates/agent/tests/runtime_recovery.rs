//! Runtime: a run taken up again after the service stopped (ADR-053). It
//! goes on from its last saved step when only reading was under way; an
//! action that may have changed something, approved actions under way, or
//! too many interruptions end it instead, without calling the model again.

mod support;

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{
    InterruptedCall, MAX_RESUMPTIONS, PendingApproval, PendingCalls, RunCheckpoint, RunOutcome,
};
use mimorya_agent::testing::{self, ScriptedStep};
use std::collections::HashMap;

use mimorya_agent::tools::ToolOverride;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use serde_json::json;
use support::{Harness, Options, setup};
use uuid::Uuid;

/// The run as saved before its first model call.
fn saved() -> RunCheckpoint {
    RunCheckpoint::new("SYSTEM".into(), vec![LlmMessage::user("Fais la synthèse")])
}

/// A call started before the stop, as the API reads it back from the run's
/// journal: with its risk when recorded.
fn call(name: &str, risk: Option<ToolRisk>) -> InterruptedCall {
    let title = match name {
        "jobs_search" => "Recherche",
        "planning_apply" => "Application du planning",
        "users_disable" => "Désactivation de compte",
        "doc_read" => "Lecture de document",
        other => other,
    };
    InterruptedCall {
        tool_name: name.into(),
        title: title.into(),
        risk,
    }
}

/// Recovers after calls recorded without their risk: judged by the tools.
async fn recover(h: &Harness, checkpoint: RunCheckpoint, interrupted: &[&str]) -> RunOutcome {
    let interrupted = interrupted.iter().map(|name| call(name, None)).collect();
    h.runtime.recover(checkpoint, interrupted).await.unwrap()
}

fn error_of(outcome: &RunOutcome) -> &str {
    match outcome {
        RunOutcome::Failed { error, .. } => error,
        other => panic!("expected a failure, got {}", other.status()),
    }
}

#[tokio::test]
async fn a_run_is_saved_before_its_first_model_call() {
    let h = setup(
        vec![ScriptedStep::text("Synthèse : rien à signaler.")],
        Options::default(),
    );
    h.start().await;
    let first = &h.ports.checkpoints()[0];
    assert_eq!(first.messages.len(), 1);
    assert_eq!(first.usage.steps, 0);
}

#[tokio::test]
async fn goes_on_from_its_last_step_when_only_reading_was_under_way() {
    let h = setup(
        vec![ScriptedStep::text("Synthèse : rien à signaler.")],
        Options::default(),
    );
    let outcome = recover(&h, saved(), &["jobs_search", "doc_read"]).await;
    assert_eq!(outcome.status(), "completed", "{outcome:?}");
    assert_eq!(
        h.events_of("run_resumed"),
        vec![json!({ "interrupted": ["jobs_search", "doc_read"] })]
    );
    // Counted, and saved before the model is called again.
    let resumed = &h.ports.checkpoints()[0];
    assert_eq!(resumed.resumptions, 1);
    assert_eq!(h.provider.requests().len(), 1);
}

#[tokio::test]
async fn never_repeats_an_action_that_may_have_changed_something() {
    let h = setup(
        vec![ScriptedStep::text("Jamais appelé")],
        Options::default(),
    );
    for interrupted in [
        vec!["jobs_search", "planning_apply"],
        vec!["tool_since_removed"],
    ] {
        let outcome = recover(&h, saved(), &interrupted).await;
        let error = error_of(&outcome);
        assert!(error.contains("son effet n’est pas connu"), "{error}");
    }
    let outcome = recover(&h, saved(), &["planning_apply"]).await;
    assert!(error_of(&outcome).contains("« Application du planning »"));
    assert!(h.provider.requests().is_empty());
    assert!(h.applied.lock().unwrap().is_empty());
    assert!(h.events_of("run_resumed").is_empty());
    let finished = h.events_of("run_finished");
    assert_eq!(finished.len(), 3);
    assert_eq!(finished[0]["status"], "failed");
}

#[tokio::test]
async fn ends_a_run_stopped_while_approved_actions_were_running() {
    let h = setup(
        vec![ScriptedStep::text("Jamais appelé")],
        Options::default(),
    );
    let mut checkpoint = saved();
    checkpoint.pending = Some(PendingCalls {
        completed_results: Vec::new(),
        approvals: vec![PendingApproval {
            approval_id: Uuid::new_v4(),
            tool_call_id: "c1".into(),
            tool_name: "planning_apply".into(),
            input: json!({ "slots": 2 }),
        }],
    });
    // The API names the actions approved among those awaited.
    let outcome = recover(&h, checkpoint.clone(), &["planning_apply"]).await;
    let error = error_of(&outcome);
    assert!(
        error.contains("l’action validée « Application du planning » : son effet n’est pas connu"),
        "{error}"
    );
    // None approved: nothing ran.
    let outcome = recover(&h, checkpoint, &[]).await;
    assert!(error_of(&outcome).contains("rien n’a été exécuté"));
    assert!(h.provider.requests().is_empty());
    assert!(h.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn names_every_action_whose_effect_is_unknown() {
    let h = setup(
        vec![ScriptedStep::text("Jamais appelé")],
        Options::default(),
    );
    let outcome = recover(
        &h,
        saved(),
        &["planning_apply", "jobs_search", "users_disable"],
    )
    .await;
    let error = error_of(&outcome);
    assert!(
        error.contains(
            "les actions « Application du planning », « Désactivation de compte » : leur effet"
        ),
        "{error}"
    );
}

#[tokio::test]
async fn judges_a_tool_by_the_risk_its_administrator_set() {
    let read = ToolOverride {
        risk: Some(ToolRisk::Read),
        ..ToolOverride::default()
    };
    let h = setup(
        vec![ScriptedStep::text("Synthèse : rien à signaler.")],
        Options {
            overrides: HashMap::from([("planning_apply".to_string(), read)]),
            ..Options::default()
        },
    );
    let outcome = recover(&h, saved(), &["planning_apply"]).await;
    assert_eq!(outcome.status(), "completed", "{outcome:?}");
}

#[tokio::test]
async fn is_not_taken_up_forever() {
    let h = setup(
        vec![ScriptedStep::text("Jamais appelé")],
        Options::default(),
    );
    let mut checkpoint = saved();
    checkpoint.resumptions = MAX_RESUMPTIONS;
    let outcome = recover(&h, checkpoint, &[]).await;
    assert!(error_of(&outcome).contains("à plusieurs reprises"));
    assert!(h.provider.requests().is_empty());
}

#[tokio::test]
async fn judges_a_call_by_the_higher_of_its_recorded_and_current_risks() {
    let h = setup(
        vec![ScriptedStep::text("Synthèse : rien à signaler.")],
        Options::default(),
    );
    // A read of a connector absent since: it only read.
    let read = vec![call("mcp_docs_search", Some(ToolRisk::Read))];
    let outcome = h.runtime.recover(saved(), read).await.unwrap();
    assert_eq!(outcome.status(), "completed", "{outcome:?}");

    for (name, recorded, title) in [
        // Recorded as a write (before its administrator made it a read).
        ("jobs_search", ToolRisk::Write, "« Recherche »"),
        // Recorded as a read, found since to write.
        (
            "planning_apply",
            ToolRisk::Read,
            "« Application du planning »",
        ),
    ] {
        let h = setup(
            vec![ScriptedStep::text("Jamais appelé")],
            Options::default(),
        );
        let interrupted = vec![call(name, Some(recorded))];
        let outcome = h.runtime.recover(saved(), interrupted).await.unwrap();
        let error = error_of(&outcome);
        assert!(
            error.contains(&format!("{title} : son effet n’est pas connu")),
            "{error}"
        );
        assert!(h.provider.requests().is_empty());
    }
}

/// Each call's name and risk, as the engine recorded them.
fn recorded_risks(h: &Harness) -> Vec<(String, Option<String>)> {
    h.events_of("tool_call")
        .iter()
        .map(|event| {
            let risk = event.get("risk").map(|risk| risk.as_str().unwrap().into());
            (event["toolName"].as_str().unwrap().into(), risk)
        })
        .collect()
}

#[tokio::test]
async fn records_the_risk_of_each_call_as_in_force_when_called() {
    let calls = vec![
        testing::call("jobs_search", json!({ "query": "chaudière" })),
        testing::call("planning_apply", json!({ "slots": 2 })),
        testing::call("users_disable", json!({ "userId": user::LUCAS })),
        testing::call("tool_unknown", json!({})),
    ];
    let h = setup(
        vec![ScriptedStep::calls("", calls), ScriptedStep::text("Fait.")],
        Options::default(),
    );
    h.start().await;
    let decisions: Vec<String> = h
        .events_of("tool_call")
        .iter()
        .map(|event| event["decision"].as_str().unwrap().into())
        .collect();
    assert_eq!(decisions, ["allow", "require_approval", "deny", "deny"]);
    let expected = [
        ("jobs_search", Some("read")),
        ("planning_apply", Some("write")),
        ("users_disable", Some("write")),
        ("tool_unknown", None),
    ];
    let expected: Vec<(String, Option<String>)> = expected
        .iter()
        .map(|(name, risk)| ((*name).into(), risk.map(Into::into)))
        .collect();
    assert_eq!(recorded_risks(&h), expected);

    // The risk its administrator set is the one recorded.
    let draft = ToolOverride {
        risk: Some(ToolRisk::Draft),
        ..ToolOverride::default()
    };
    let search = testing::call("jobs_search", json!({ "query": "chaudière" }));
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![search]),
            ScriptedStep::text("Fait."),
        ],
        Options {
            overrides: HashMap::from([("jobs_search".to_string(), draft)]),
            ..Options::default()
        },
    );
    h.start().await;
    assert_eq!(
        recorded_risks(&h),
        [("jobs_search".to_string(), Some("draft".to_string()))]
    );
}
