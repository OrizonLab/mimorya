//! Runtime: the company's guard of the actions (ADR-213, D5). Asked of each
//! call with a side effect the policy allows or puts to validation, it can
//! only add: an action allowed alone that it signals, or cannot read, waits
//! for a person; one already put to validation keeps it, its signals told
//! to whoever decides. A read, a denied call and a call a person validated
//! are never asked; what a person validated runs as validated.

mod support;

use std::collections::{HashMap, HashSet};

use mimorya_agent::runtime::{ApprovalDecision, GuardVerdict, RunOutcome};
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use serde_json::json;
use support::{Options, setup};

/// A company that lets `planning_apply` run without validation.
fn autonomous() -> Options {
    Options {
        autonomy: Some(AutonomyPolicy {
            level: AutonomyLevel::AutonomousLimited,
            autonomous_tools: HashSet::from(["planning_apply".to_string()]),
        }),
        ..Options::default()
    }
}

fn apply_then_answer() -> Vec<ScriptedStep> {
    vec![
        ScriptedStep::calls(
            "",
            vec![
                call("jobs_search", json!({ "query": "x" })),
                call("planning_apply", json!({ "slots": 2 })),
            ],
        ),
        ScriptedStep::text("Planning appliqué."),
    ]
}

#[tokio::test]
async fn an_action_allowed_alone_runs_when_the_guard_says_nothing() {
    let h = setup(apply_then_answer(), autonomous());
    let outcome = h.start().await;
    assert_eq!(outcome.status(), "completed");
    assert_eq!(*h.applied.lock().unwrap(), vec![json!({ "slots": 2 })]);
    // The read was never asked; the action was.
    assert_eq!(h.ports.guarded(), ["planning_apply"]);
}

#[tokio::test]
async fn an_action_allowed_alone_waits_for_a_person_when_signalled_or_unread() {
    for (verdict, said) in [
        (
            GuardVerdict::Signals(vec!["destinataire externe".into(), "montant élevé".into()]),
            "Validation demandée par le service de décision : destinataire externe, montant élevé",
        ),
        (
            GuardVerdict::Unread,
            "Validation demandée : le service de décision n’a pas pu se prononcer",
        ),
    ] {
        let h = setup(apply_then_answer(), autonomous());
        h.ports.guard_says(verdict);
        let RunOutcome::AwaitingApproval {
            approvals,
            checkpoint,
        } = h.start().await
        else {
            panic!("expected a pause")
        };
        assert!(h.applied.lock().unwrap().is_empty());
        let (_, request) = &h.ports.approval_requests()[0];
        assert_eq!(request.tool_name, "planning_apply");
        assert_eq!(request.reason, said);
        // Validated, it runs as validated, without asking the guard again.
        let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
        let resumed = h.runtime.resume(checkpoint, &decisions).await.unwrap();
        assert_eq!(resumed.status(), "completed");
        assert_eq!(*h.applied.lock().unwrap(), vec![json!({ "slots": 2 })]);
        assert_eq!(h.ports.guarded(), ["planning_apply"]);
    }
}

#[tokio::test]
async fn an_action_already_put_to_validation_keeps_it_with_the_signals_told() {
    // Signalled: the reason says so; unread: it says nothing more.
    for (verdict, said) in [
        (
            GuardVerdict::Signals(vec!["données personnelles".into()]),
            "Action modifiant des données · Signalé par le service de décision : données \
             personnelles",
        ),
        (GuardVerdict::Unread, "Action modifiant des données"),
        (GuardVerdict::Clear, "Action modifiant des données"),
    ] {
        let h = setup(apply_then_answer(), Options::default());
        h.ports.guard_says(verdict);
        let outcome = h.start().await;
        assert_eq!(outcome.status(), "awaiting_approval");
        let (_, request) = &h.ports.approval_requests()[0];
        assert_eq!(request.reason, said);
    }
}

#[tokio::test]
async fn a_denied_call_is_never_asked_nor_lifted() {
    let mut options = autonomous();
    // Without the permission its tools require, each call is denied.
    options.allowed_permissions = Some(HashSet::new());
    let h = setup(apply_then_answer(), options);
    h.ports
        .guard_says(GuardVerdict::Signals(vec!["risque élevé".into()]));
    let outcome = h.start().await;
    assert_eq!(outcome.status(), "completed");
    assert!(h.applied.lock().unwrap().is_empty());
    assert!(h.ports.approval_requests().is_empty());
    assert!(h.ports.guarded().is_empty());
}
