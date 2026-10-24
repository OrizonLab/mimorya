//! Runtime: a mission's stage (ADR-089). Its budget spent, it ends with the
//! progress note the model writes, never running a tool called instead, and
//! only while its user may still act; the sources earlier stages collected
//! ground its answer.

mod support;

use mimorya_agent::llm::LlmMessage;
use std::sync::Arc;

use mimorya_agent::runtime::{
    ACCOUNT_INACTIVE, BudgetLimits, CurrentAccess, DEFAULT_BUDGET, NO_NOTE, RunOutcome,
};
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, TestAccess, call};
use mimorya_core::demo::{self, user};
use mimorya_core::tools::Citation;
use serde_json::json;
use support::{Options, setup};

fn search() -> ScriptedStep {
    ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))])
}

fn stage(steps: Vec<ScriptedStep>) -> support::Harness {
    setup(
        steps,
        Options {
            budget: Some(BudgetLimits {
                max_steps: 2,
                ..DEFAULT_BUDGET
            }),
            handoff: true,
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn a_stage_whose_budget_is_spent_ends_with_its_progress_note() {
    let h = stage(vec![
        search(),
        search(),
        ScriptedStep::dynamic(|request| {
            let Some(LlmMessage::User { content }) = request.last_message() else {
                panic!("expected the note to be asked for")
            };
            assert!(content.contains("nombre maximal d’étapes atteint (2)"));
            assert!(content.contains("N’appelle plus aucun outil"));
            // Still declared: the conversation holds their calls.
            assert!(request.tool_names().contains(&"jobs_search".to_string()));
            ScriptedReply::text("Fait : INT-0001 relue. Reste : INT-0002.")
        }),
    ]);
    let RunOutcome::Handoff { note, checkpoint } = h.start().await else {
        panic!("expected a handoff")
    };
    assert_eq!(note, "Fait : INT-0001 relue. Reste : INT-0002.");
    assert_eq!(checkpoint.usage.steps, 3);
    assert_eq!(h.provider.remaining(), 0);
    let finished: Vec<_> = h.ports.finished().iter().map(|f| f.status).collect();
    assert_eq!(finished, vec!["completed"]);
    assert_eq!(
        h.events_of("assistant_message"),
        vec![json!({ "text": note, "final": true })]
    );
    assert_eq!(
        h.events_of("run_finished"),
        vec![json!({ "status": "completed" })]
    );
}

#[tokio::test]
async fn a_tool_called_instead_of_the_note_is_never_run() {
    let h = stage(vec![
        search(),
        search(),
        ScriptedStep::calls("", vec![call("planning_apply", json!({ "slots": 2 }))]),
    ]);
    let RunOutcome::Handoff { note, .. } = h.start().await else {
        panic!("expected a handoff")
    };
    assert_eq!(note, NO_NOTE);
    assert!(h.applied.lock().unwrap().is_empty());
    assert!(h.ports.approval_requests().is_empty());
    let called: Vec<_> = h
        .ports
        .tool_records()
        .into_iter()
        .map(|r| r.tool_name)
        .collect();
    assert_eq!(called, vec!["jobs_search", "jobs_search"]);
}

#[tokio::test]
async fn a_stage_whose_user_may_no_longer_act_writes_no_note() {
    let julien = || {
        Some(CurrentAccess {
            engine: Arc::new(demo::authorization_engine()),
            subject: Arc::new(demo::subject(user::JULIEN)),
        })
    };
    // Read before each of the two model calls and after their tool calls;
    // gone when the note would be asked for.
    let access = TestAccess::sequence(vec![julien(), julien(), julien(), julien(), None]);
    let h = setup(
        vec![search(), search()],
        Options {
            budget: Some(BudgetLimits {
                max_steps: 2,
                ..DEFAULT_BUDGET
            }),
            handoff: true,
            access: Some(access),
            ..Default::default()
        },
    );
    let RunOutcome::Failed { error, .. } = h.start().await else {
        panic!("expected a failure")
    };
    assert_eq!(error, ACCOUNT_INACTIVE);
    assert_eq!(h.provider.requests().len(), 2);
    assert_eq!(h.events_of("run_finished")[0]["status"], "failed");
}

#[tokio::test]
async fn the_sources_of_earlier_stages_ground_an_answer() {
    let h = stage(vec![ScriptedStep::text("INT-0002 : fuite réparée.")]);
    let earlier = vec![Citation::new("intervention", "INT-0002")];
    let RunOutcome::Completed { verification, .. } = h.start_with_evidence(earlier).await else {
        panic!("expected completion")
    };
    assert!(verification.passed, "{:?}", verification.issues);
}
