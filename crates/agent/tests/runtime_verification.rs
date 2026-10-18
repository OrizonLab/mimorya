//! Runtime: verification of final answers, bounded correction and budgets.

mod support;

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{DEFAULT_BUDGET, RunOutcome, VerificationReport};
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call};
use serde_json::json;
use support::{Options, setup};

#[tokio::test]
async fn detects_ungrounded_identifiers_and_corrects_the_answer() {
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]),
            ScriptedStep::text("Interventions : INT-0001 et INT-0999."),
            ScriptedStep::dynamic(|req| {
                let Some(LlmMessage::User { content }) = req.last_message() else {
                    panic!("expected a correction")
                };
                assert!(content.contains("INT-0999 est cité"));
                ScriptedReply::text("Intervention : INT-0001.")
            }),
        ],
        Options::default(),
    );
    let RunOutcome::Completed { verification, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert!(verification.passed);
    assert_eq!(h.provider.remaining(), 0);
    let passes: Vec<bool> = h
        .events_of("verification")
        .iter()
        .map(|v| v["passed"].as_bool().unwrap())
        .collect();
    assert_eq!(passes, vec![false, true]);
}

#[tokio::test]
async fn never_hides_residual_verification_issues() {
    let h = setup(
        vec![
            ScriptedStep::text("INT-0999"),
            ScriptedStep::text("INT-0999 toujours"),
            ScriptedStep::text("INT-0999 encore"),
        ],
        Options::default(),
    );
    let RunOutcome::Completed {
        answer,
        verification,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert!(!verification.passed);
    assert!(answer.contains("Points non vérifiés"));
}

#[tokio::test]
async fn uses_the_model_review_against_explicit_success_criteria() {
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]),
            ScriptedStep::text("INT-0001."),
            ScriptedStep::text("INT-0001 : chaudière remise en service, aucun suivi nécessaire."),
        ],
        Options {
            criteria: vec!["Indiquer pour chaque intervention si un suivi est nécessaire".into()],
            judge: Some(vec![
                ScriptedStep::calls(
                    "",
                    vec![call(
                        "submit_verdict",
                        json!({ "pass": false, "issues": ["Suivi non précisé"] }),
                    )],
                ),
                ScriptedStep::calls(
                    "",
                    vec![call(
                        "submit_verdict",
                        json!({ "pass": true, "issues": [] }),
                    )],
                ),
            ]),
            ..Default::default()
        },
    );
    let RunOutcome::Completed { verification, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert_eq!(
        verification,
        VerificationReport {
            passed: true,
            issues: vec![],
            judged: true
        }
    );
    let judge = h.judge.as_ref().unwrap();
    assert!(matches!(
        judge.requests()[0].messages[0],
        LlmMessage::User { .. }
    ));
}

#[tokio::test]
async fn stops_when_the_step_budget_is_exhausted() {
    let looping = || ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]);
    let h = setup(
        vec![looping(), looping(), looping(), looping()],
        Options {
            budget: Some(mimorya_agent::runtime::BudgetLimits {
                max_steps: 2,
                ..DEFAULT_BUDGET
            }),
            ..Default::default()
        },
    );
    let RunOutcome::Failed { error, .. } = h.start().await else {
        panic!("expected failure")
    };
    assert!(error.contains("nombre maximal d’étapes"));
    assert_eq!(
        h.events_of("run_finished"),
        vec![json!({ "status": "failed", "error": error })]
    );
}
