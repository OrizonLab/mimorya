//! Runtime: a run taken up again after the service stopped (ADR-053). It
//! goes on from its last saved step when only reading was under way; an
//! action that may have changed something, approved actions under way, or
//! too many interruptions end it instead, without calling the model again.

mod support;

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{
    MAX_RESUMPTIONS, PendingApproval, PendingCalls, RunCheckpoint, RunOutcome,
};
use mimorya_agent::testing::ScriptedStep;
use serde_json::json;
use support::{Harness, Options, setup};
use uuid::Uuid;

/// The run as saved before its first model call.
fn saved() -> RunCheckpoint {
    RunCheckpoint::new("SYSTEM".into(), vec![LlmMessage::user("Fais la synthèse")])
}

async fn recover(h: &Harness, checkpoint: RunCheckpoint, interrupted: &[&str]) -> RunOutcome {
    let interrupted = interrupted.iter().map(|name| name.to_string()).collect();
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
    let outcome = recover(&h, checkpoint, &[]).await;
    let error = error_of(&outcome);
    assert!(
        error.contains("actions validées (« Application du planning »)"),
        "{error}"
    );
    assert!(h.provider.requests().is_empty());
    assert!(h.applied.lock().unwrap().is_empty());
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
