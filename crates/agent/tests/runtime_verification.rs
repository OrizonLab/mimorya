//! Runtime: verification of final answers, bounded correction and budgets;
//! a review listing many issues fails the answer with the first ones (ADR-092);
//! a review model that fails leaves the answer to the agent's, unjudged only
//! when both fail (ADR-108).

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
async fn a_review_listing_many_issues_fails_the_answer_with_the_first_ones() {
    let many: Vec<String> = (1..=15).map(|i| format!("Manquement {i}")).collect();
    let failing = || {
        ScriptedStep::calls(
            "",
            vec![call(
                "submit_verdict",
                json!({ "pass": false, "issues": many.clone() }),
            )],
        )
    };
    let h = setup(
        vec![
            ScriptedStep::text("Rien à signaler."),
            ScriptedStep::dynamic(|req| {
                let Some(LlmMessage::User { content }) = req.last_message() else {
                    panic!("expected a correction")
                };
                assert!(content.contains("- Manquement 10\n"), "{content}");
                assert!(!content.contains("Manquement 11"), "{content}");
                assert!(content.contains("Et 5 autres manquements"), "{content}");
                ScriptedReply::text("Toujours rien à signaler.")
            }),
            ScriptedStep::text("Rien, vraiment."),
        ],
        Options {
            criteria: vec!["Indiquer pour chaque intervention si un suivi est nécessaire".into()],
            judge: Some(vec![failing(), failing(), failing()]),
            ..Default::default()
        },
    );
    let RunOutcome::Completed {
        answer,
        verification,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    // Heard, not set aside: the answer fails, and says why.
    assert!(verification.judged);
    assert!(!verification.passed);
    assert_eq!(verification.issues.len(), 11);
    assert!(answer.contains("Points non vérifiés"));
    assert!(answer.contains("Et 5 autres manquements signalés"));
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

#[tokio::test]
async fn a_failing_review_model_leaves_the_answer_to_the_agent_s_then_unjudged() {
    let criteria = vec!["Indiquer pour chaque intervention si un suivi est nécessaire".into()];
    // The agent's model judges in place of the review model that fails.
    let h = setup(
        vec![
            ScriptedStep::text("Rien à signaler."),
            ScriptedStep::calls("", vec![call("submit_verdict", json!({ "pass": true }))]),
        ],
        Options {
            criteria: criteria.clone(),
            judge: Some(vec![ScriptedStep::fails("Modèle inconnu.")]),
            ..Default::default()
        },
    );
    let RunOutcome::Completed { verification, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert!(verification.judged && verification.passed);
    assert_eq!(h.judge.as_ref().unwrap().requests().len(), 1);
    assert_eq!(h.provider.requests()[1].tool_names(), ["submit_verdict"]);

    // Both failing: the answer stands, unjudged, after one call to each.
    let h = setup(
        vec![
            ScriptedStep::text("Rien à signaler."),
            ScriptedStep::fails("Surchargé."),
        ],
        Options {
            criteria,
            judge: Some(vec![ScriptedStep::fails("Modèle inconnu.")]),
            ..Default::default()
        },
    );
    let RunOutcome::Completed {
        answer,
        verification,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, "Rien à signaler.");
    assert!(!verification.judged && verification.passed);
    assert_eq!(h.judge.as_ref().unwrap().requests().len(), 1);
    assert_eq!(h.provider.requests().len(), 2);
}
