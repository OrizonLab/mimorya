//! Runtime: the user's rights are read again before each model call and
//! before the tool calls it returns (ADR-035). An account disabled during the
//! run ends it there, before any other model call or action; a right
//! withdrawn no longer counts, neither for the tools offered to the model nor
//! for the calls it still makes; a run resumed after validation acts with the
//! rights of that moment.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use mimorya_agent::runtime::{ACCOUNT_INACTIVE, ApprovalDecision, CurrentAccess, RunOutcome};
use mimorya_agent::testing::{ScriptedStep, TestAccess, call};
use mimorya_core::authz::Subject;
use mimorya_core::demo::{self, user};
use serde_json::json;
use support::{Options, setup};

fn access(subject: Subject) -> Option<CurrentAccess> {
    Some(CurrentAccess {
        engine: Arc::new(demo::authorization_engine()),
        subject: Arc::new(subject),
    })
}

/// Julien without any role assignment: his rights were all withdrawn.
fn stripped() -> Subject {
    Subject {
        assignments: Vec::new(),
        ..demo::subject(user::JULIEN)
    }
}

fn failed_with(outcome: RunOutcome) -> String {
    match outcome {
        RunOutcome::Failed { error, .. } => error,
        other => panic!("expected a failed run, got {}", other.status()),
    }
}

#[tokio::test]
async fn an_account_disabled_between_two_steps_ends_the_run_there() {
    let julien = || access(demo::subject(user::JULIEN));
    let h = setup(
        vec![
            ScriptedStep::calls(
                "Je cherche.",
                vec![call("jobs_search", json!({ "query": "a" }))],
            ),
            ScriptedStep::calls(
                "Encore.",
                vec![call("jobs_search", json!({ "query": "b" }))],
            ),
            ScriptedStep::text("Synthèse."),
        ],
        Options {
            // Read before the first model call, before its tool calls, then
            // before the second model call.
            access: Some(TestAccess::sequence(vec![julien(), julien(), None])),
            ..Options::default()
        },
    );

    assert_eq!(failed_with(h.start().await), ACCOUNT_INACTIVE);
    // One model call, one action: nothing after the account was disabled.
    assert_eq!(h.provider.requests().len(), 1);
    assert_eq!(h.provider.remaining(), 2);
    assert_eq!(h.ports.tool_records().len(), 1);
    let finished = h.events_of("run_finished");
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0]["status"], "failed");
    assert_eq!(finished[0]["error"], ACCOUNT_INACTIVE);
}

#[tokio::test]
async fn an_account_disabled_while_the_model_answers_runs_none_of_its_calls() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "J’applique.",
                vec![call("planning_apply", json!({ "slots": 2 }))],
            ),
            ScriptedStep::text("Planning appliqué."),
        ],
        Options {
            access: Some(TestAccess::sequence(vec![
                access(demo::subject(user::JULIEN)),
                None,
            ])),
            ..Options::default()
        },
    );

    assert_eq!(failed_with(h.start().await), ACCOUNT_INACTIVE);
    assert!(h.ports.tool_records().is_empty());
    assert!(h.ports.approval_requests().is_empty());
    assert!(h.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_right_withdrawn_during_the_run_no_longer_counts() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "Je cherche.",
                vec![call("jobs_search", json!({ "query": "a" }))],
            ),
            ScriptedStep::calls(
                "Encore.",
                vec![call("jobs_search", json!({ "query": "b" }))],
            ),
            ScriptedStep::text("Je ne peux plus consulter les interventions."),
        ],
        Options {
            access: Some(TestAccess::sequence(vec![
                access(demo::subject(user::JULIEN)),
                access(demo::subject(user::JULIEN)),
                access(stripped()),
            ])),
            ..Options::default()
        },
    );

    h.start().await;
    let requests = h.provider.requests();
    assert!(
        requests[0]
            .tool_names()
            .contains(&"jobs_search".to_string())
    );
    // The tool is no longer offered, and the call the model still makes is refused.
    assert!(
        !requests[1]
            .tool_names()
            .contains(&"jobs_search".to_string())
    );
    let records = h.ports.tool_records();
    let statuses: Vec<&str> = records.iter().map(|r| r.status).collect();
    assert_eq!(statuses, ["succeeded", "denied"]);
    assert_eq!(
        records[1].decision_reason.as_deref(),
        Some("Permission intervention:read requise")
    );
}

#[tokio::test]
async fn a_run_resumed_once_its_account_was_disabled_executes_nothing() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "J’applique.",
                vec![call("planning_apply", json!({ "slots": 2 }))],
            ),
            ScriptedStep::text("Planning appliqué."),
        ],
        Options {
            access: Some(TestAccess::sequence(vec![
                access(demo::subject(user::JULIEN)),
                access(demo::subject(user::JULIEN)),
                None,
            ])),
            ..Options::default()
        },
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause for validation")
    };

    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let outcome = h.runtime.resume(checkpoint, &decisions).await.unwrap();
    assert_eq!(failed_with(outcome), ACCOUNT_INACTIVE);
    assert!(h.applied.lock().unwrap().is_empty());
    assert_eq!(h.provider.remaining(), 1);
}

#[tokio::test]
async fn an_approved_call_runs_with_the_rights_of_its_resumption() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "J’applique.",
                vec![call("planning_apply", json!({ "slots": 2 }))],
            ),
            ScriptedStep::text("Je n’ai pas pu appliquer le planning."),
        ],
        Options {
            access: Some(TestAccess::sequence(vec![
                access(demo::subject(user::JULIEN)),
                access(demo::subject(user::JULIEN)),
                access(stripped()),
            ])),
            ..Options::default()
        },
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause for validation")
    };

    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    h.runtime.resume(checkpoint, &decisions).await.unwrap();
    // Approved, but the right was withdrawn in between: refused, not applied.
    assert!(h.applied.lock().unwrap().is_empty());
    let applied = h
        .ports
        .tool_records()
        .into_iter()
        .rfind(|r| r.tool_name == "planning_apply")
        .expect("planning_apply record");
    assert_eq!(applied.status, "failed");
}
