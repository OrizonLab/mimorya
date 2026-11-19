//! Escalation after a failed review (ADR-207): once the main model's
//! corrections fail the review, the run goes on from where it stands with
//! the escalation model, told what the review found, once a run; nothing
//! done is done again. Past the month's budget or the run's escalations, or
//! should the escalation model fail, the main model goes on and the event
//! is journaled. A run taken up from its checkpoint stays escalated, unless
//! its company's escalation is gone by then: it goes on with its main
//! model. An escalated call is priced as the model asked for when its
//! answer names one the platform has no price for, and so is a review's.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmMessage, ModelRole};
use mimorya_agent::runtime::{
    ApprovalDecision, Escalation, EscalationBudget, RunCheckpoint, RunOutcome,
};
use mimorya_agent::testing::{ScriptedProvider, ScriptedStep, call};
use serde_json::json;
use support::renamed::Renamed;
use support::{Harness, Options, setup};

struct Budget(bool);

#[async_trait]
impl EscalationBudget for Budget {
    async fn allows(&self) -> anyhow::Result<bool> {
        Ok(self.0)
    }
}

fn escalation(
    steps: Vec<ScriptedStep>,
    max_per_run: u32,
    allows: bool,
) -> (Escalation, Arc<ScriptedProvider>) {
    let provider = Arc::new(ScriptedProvider::new(steps));
    let escalation = Escalation {
        provider: provider.clone(),
        after_failure: true,
        router: None,
        advisor: None,
        max_per_run,
        budget: Arc::new(Budget(allows)),
    };
    (escalation, provider)
}

/// The main model searches once, then cites an intervention the search did
/// not return, and keeps doing so through its two corrections.
fn failing_main_model() -> Vec<ScriptedStep> {
    vec![
        ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]),
        ScriptedStep::text("INT-0999"),
        ScriptedStep::text("INT-0999 toujours"),
        ScriptedStep::text("INT-0999 encore"),
    ]
}

fn run_with(main: Vec<ScriptedStep>, escalation: Escalation) -> Harness {
    setup(
        main,
        Options {
            escalation: Some(escalation),
            ..Options::default()
        },
    )
}

fn roles(h: &Harness) -> Vec<ModelRole> {
    h.ports.usage_records().iter().map(|r| r.role).collect()
}

fn audited(h: &Harness, action: &str) -> Vec<Option<String>> {
    h.ports
        .audit_entries()
        .into_iter()
        .filter(|entry| entry.action == action)
        .map(|entry| entry.reason)
        .collect()
}

fn searches(h: &Harness) -> usize {
    h.ports
        .tool_records()
        .iter()
        .filter(|record| record.tool_name == "jobs_search" && record.status == "succeeded")
        .count()
}

#[tokio::test]
async fn a_failed_review_hands_the_run_to_the_escalation_model_from_where_it_stands() {
    let (escalation, stronger) = escalation(vec![ScriptedStep::text("INT-0001.")], 5, true);
    let h = run_with(failing_main_model(), escalation);
    let RunOutcome::Completed {
        answer,
        verification,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert!(verification.passed, "{verification:?}");
    assert_eq!(answer, "INT-0001.");
    // Told what the review found, with all the run did before.
    let requests = stronger.requests();
    assert_eq!(requests.len(), 1);
    let Some(LlmMessage::User { content }) = requests[0].last_message() else {
        panic!("expected the platform's message")
    };
    assert!(content.contains("INT-0999"), "{content}");
    assert!(content.contains("ne les refais pas"), "{content}");
    assert!(
        requests[0]
            .messages
            .iter()
            .any(|m| matches!(m, LlmMessage::ToolResults { .. })),
        "the search's results were not handed over"
    );
    // The search ran once; the main model's calls and the escalated one
    // are told apart.
    assert_eq!(searches(&h), 1);
    assert_eq!(
        roles(&h),
        [[ModelRole::Main; 4].as_slice(), &[ModelRole::Escalation]].concat()
    );
    assert_eq!(
        audited(&h, "model_escalation.started"),
        [Some("after_failure".into())]
    );
    assert!(checkpoint.escalation.escalated && checkpoint.escalation.after_failure_used);
    assert_eq!(checkpoint.escalation.calls, 1);
    // The escalated answer's review is an attempt of its own.
    let attempts: Vec<_> = h
        .events_of("verification")
        .iter()
        .map(|event| event["attempt"].as_u64().unwrap())
        .collect();
    assert_eq!(attempts, [1, 2, 3, 4]);
}

#[tokio::test]
async fn an_escalated_answer_failing_again_is_shown_with_its_warnings_once_escalated() {
    let (escalation, stronger) = escalation(
        vec![
            ScriptedStep::text("INT-0999 de nouveau"),
            ScriptedStep::text("jamais demandé"),
        ],
        5,
        true,
    );
    let h = run_with(failing_main_model(), escalation);
    let RunOutcome::Completed {
        answer,
        verification,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert!(!verification.passed);
    assert!(answer.contains("Points non vérifiés"), "{answer}");
    assert_eq!(stronger.requests().len(), 1, "escalated more than once");
    assert_eq!(audited(&h, "model_escalation.started").len(), 1);
}

#[tokio::test]
async fn past_the_month_s_budget_the_main_model_s_answer_stands_and_it_is_journaled() {
    let (escalation, stronger) = escalation(vec![ScriptedStep::text("jamais demandé")], 5, false);
    let h = run_with(failing_main_model(), escalation);
    let RunOutcome::Completed { verification, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert!(!verification.passed);
    assert!(stronger.requests().is_empty());
    assert_eq!(
        audited(&h, "model_escalation.limited"),
        [Some("monthly_budget".into())]
    );
    assert!(!roles(&h).contains(&ModelRole::Escalation));
}

#[tokio::test]
async fn past_the_run_s_escalations_the_main_model_takes_over() {
    let mut main = failing_main_model();
    main.push(ScriptedStep::text("INT-0001, d’après la recherche."));
    let (escalation, stronger) = escalation(
        vec![ScriptedStep::calls(
            "",
            vec![call("jobs_search", json!({ "query": "y" }))],
        )],
        1,
        true,
    );
    let h = run_with(main, escalation);
    let RunOutcome::Completed {
        answer, checkpoint, ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, "INT-0001, d’après la recherche.");
    assert_eq!(stronger.requests().len(), 1);
    assert_eq!(h.provider.remaining(), 0);
    assert_eq!(
        audited(&h, "model_escalation.limited"),
        [Some("per_run".into())]
    );
    assert!(!checkpoint.escalation.escalated);
}

#[tokio::test]
async fn an_escalation_model_that_fails_hands_back_to_the_main_model() {
    let mut main = failing_main_model();
    main.push(ScriptedStep::text("INT-0001."));
    // The call and its two retries fail.
    let (escalation, stronger) = escalation(
        (0..3).map(|_| ScriptedStep::fails("surchargé")).collect(),
        5,
        true,
    );
    let h = run_with(main, escalation);
    let RunOutcome::Completed {
        answer,
        verification,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, "INT-0001.");
    assert!(verification.passed);
    assert_eq!(stronger.remaining(), 0);
    assert_eq!(audited(&h, "model_escalation.failed").len(), 1);
    assert_eq!(roles(&h).last(), Some(&ModelRole::Main));
}

#[tokio::test]
async fn a_run_taken_up_from_its_checkpoint_stays_on_the_escalation_model() {
    let (escalation, stronger) = escalation(
        vec![
            ScriptedStep::calls("", vec![call("planning_apply", json!({ "slots": 2 }))]),
            ScriptedStep::text("INT-0001 : deux créneaux réservés."),
        ],
        5,
        true,
    );
    let h = run_with(failing_main_model(), escalation);
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause for validation")
    };
    // As another process reads it back.
    let stored = serde_json::to_value(&checkpoint).unwrap();
    let checkpoint: RunCheckpoint = serde_json::from_value(stored).unwrap();
    assert!(checkpoint.escalation.escalated);
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let outcome = h.runtime.resume(checkpoint, &decisions).await.unwrap();
    assert_eq!(outcome.status(), "completed");
    assert_eq!(stronger.requests().len(), 2);
    assert_eq!(h.provider.remaining(), 0);
    assert_eq!(*h.applied.lock().unwrap(), vec![json!({ "slots": 2 })]);
}

#[tokio::test]
async fn a_run_taken_up_once_its_company_s_escalation_is_gone_goes_on_with_its_main_model() {
    let (escalation, stronger) = escalation(
        vec![ScriptedStep::calls(
            "",
            vec![call("planning_apply", json!({ "slots": 2 }))],
        )],
        5,
        true,
    );
    let h = run_with(failing_main_model(), escalation);
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause for validation")
    };
    assert!(checkpoint.escalation.escalated);
    // Taken up where the company's escalation was removed meanwhile.
    let taken_up = setup(
        vec![ScriptedStep::text("INT-0001 : deux créneaux réservés.")],
        Options::default(),
    );
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let RunOutcome::Completed {
        answer, checkpoint, ..
    } = taken_up
        .runtime
        .resume(checkpoint, &decisions)
        .await
        .unwrap()
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, "INT-0001 : deux créneaux réservés.");
    assert!(!checkpoint.escalation.escalated);
    assert_eq!(stronger.requests().len(), 1);
    assert_eq!(roles(&taken_up), [ModelRole::Main]);
    assert_eq!(
        *taken_up.applied.lock().unwrap(),
        vec![json!({ "slots": 2 })]
    );
}

#[tokio::test]
async fn without_the_mechanism_turned_on_nothing_escalates() {
    let (mut escalation, stronger) = escalation(vec![ScriptedStep::text("jamais")], 5, true);
    escalation.after_failure = false;
    let h = run_with(failing_main_model(), escalation);
    let RunOutcome::Completed { verification, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert!(!verification.passed);
    assert!(stronger.requests().is_empty());
    assert!(
        h.ports
            .audit_entries()
            .iter()
            .all(|e| !e.action.starts_with("model_escalation"))
    );
}

#[tokio::test]
async fn an_escalated_call_is_priced_as_the_model_asked_for_when_its_answer_names_another() {
    let (mut escalation, stronger) = escalation(vec![ScriptedStep::text("INT-0001.")], 5, true);
    escalation.provider = Arc::new(Renamed(stronger));
    let h = run_with(failing_main_model(), escalation);
    h.start().await;
    let records = h.ports.usage_records();
    let escalated = records
        .iter()
        .find(|r| r.role == ModelRole::Escalation)
        .unwrap();
    // The scripted model names itself, unpriced; priced as claude-opus-4-8.
    assert_eq!(escalated.model, "scripted-model");
    assert!(escalated.cost_micros > 0, "{escalated:?}");
    assert!(
        records
            .iter()
            .filter(|r| r.role == ModelRole::Main)
            .all(|r| r.cost_micros == 0)
    );
}

#[tokio::test]
async fn a_review_is_priced_as_the_model_asked_for_when_its_answer_names_another() {
    let judge = Arc::new(ScriptedProvider::new(vec![ScriptedStep::calls(
        "",
        vec![call(
            "submit_verdict",
            json!({ "pass": true, "issues": [] }),
        )],
    )]));
    let h = setup(
        vec![ScriptedStep::text("Trois interventions cette semaine.")],
        Options {
            criteria: vec!["Dire combien.".into()],
            judge_model: Some(Arc::new(Renamed(judge.clone()))),
            llm_judge: Some(true),
            ..Options::default()
        },
    );
    let RunOutcome::Completed { verification, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert!(verification.judged, "{verification:?}");
    let records = h.ports.usage_records();
    let review = records
        .iter()
        .find(|r| r.role == ModelRole::Review)
        .unwrap();
    assert_eq!(review.model, "scripted-model");
    assert!(review.cost_micros > 0, "{review:?}");
}
