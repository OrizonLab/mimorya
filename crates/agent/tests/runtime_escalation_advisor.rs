//! Anthropic's advisor (ADR-207, step 3): decided at a run's first call,
//! given unchanged to the calls after while the run's bounds allow; each
//! consultation is an escalation, recorded as the advisor's and priced as
//! its model. A turn paused while it is consulted goes on from the paused
//! answer, with it whatever the bounds. Past a bound it is withdrawn for
//! good, journaled once, and the reasoning bound to it dropped. A run the
//! router starts on the escalation model, or begun past the month's budget,
//! has none; a run taken up once its company withdrew it goes on without.
//! Turns, escalations and pauses: `runtime_escalation_advisor_turns.rs`.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use mimorya_agent::llm::{EscalationRoute, LlmMessage, ModelRole};
use mimorya_agent::runtime::{AdvisorState, ApprovalDecision, Escalation, RunOutcome};
use mimorya_agent::testing::{ScriptedProvider, ScriptedReply, ScriptedStep, call};
use serde_json::json;
use support::advisor::{ADVISOR, Budget, advised, advisors, given, limited, run_advised};
use support::{Harness, Options, setup};

const ANSWER: &str = "Synthèse : rien à signaler cette semaine.";

fn search() -> Vec<mimorya_agent::testing::ScriptedCall> {
    vec![call("jobs_search", json!({ "query": "x" }))]
}

fn roles(h: &Harness) -> Vec<ModelRole> {
    h.ports.usage_records().iter().map(|r| r.role).collect()
}

#[tokio::test]
async fn the_main_model_consults_its_advisor_counted_as_an_escalation() {
    let (escalation, stronger) = advised(5, true, Vec::new());
    let h = run_advised(
        vec![
            ScriptedStep::Fixed(ScriptedReply::calls("", search()).consulting(ADVISOR)),
            ScriptedStep::text(ANSWER),
        ],
        escalation,
    );
    let RunOutcome::Completed {
        answer, checkpoint, ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, ANSWER);
    // The same advisor in every call; the escalation model never called.
    assert_eq!(advisors(&h), vec![(given(5), false), (given(5), false)]);
    assert!(stronger.requests().is_empty());
    assert_eq!(checkpoint.escalation.calls, 1);
    assert_eq!(
        checkpoint.escalation.advisor,
        AdvisorState::Given(given(5).unwrap())
    );
    // Recorded as the advisor's, priced as its model.
    assert_eq!(
        roles(&h),
        [ModelRole::Main, ModelRole::Advisor, ModelRole::Main]
    );
    let consulted = &h.ports.usage_records()[1];
    assert_eq!(consulted.model, ADVISOR);
    assert!(!consulted.by_runs_model);
    assert_eq!(consulted.cost_micros, 12_000);
    assert_eq!(
        (consulted.input_tokens, consulted.output_tokens),
        (1_000, 400)
    );
    assert_eq!(checkpoint.usage.cost_micros, 12_000);
    assert_eq!(h.events_of("usage").len(), 3);
}

#[tokio::test]
async fn a_turn_paused_while_its_advisor_is_consulted_goes_on_from_the_paused_answer() {
    let (escalation, _) = advised(5, true, Vec::new());
    let h = run_advised(
        vec![
            ScriptedStep::Fixed(ScriptedReply::text("Je consulte.").paused()),
            ScriptedStep::dynamic(|request| {
                // Nothing added after the paused answer.
                let Some(LlmMessage::Assistant { content, .. }) = request.last_message() else {
                    panic!("expected the paused answer last");
                };
                assert_eq!(content, "Je consulte.");
                ScriptedReply::text(ANSWER).consulting(ADVISOR)
            }),
        ],
        escalation,
    );
    let RunOutcome::Completed {
        answer, checkpoint, ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, ANSWER);
    assert_eq!(advisors(&h), vec![(given(5), false), (given(5), false)]);
    assert!(!checkpoint.escalation.advisor_pending);
    let said: Vec<_> = h.events_of("assistant_message");
    assert_eq!(said[0]["text"], "Je consulte.");
    assert_eq!(said[0]["final"], false);
}

#[tokio::test]
async fn past_the_run_s_escalations_the_advisor_is_withdrawn_for_good() {
    let (escalation, _) = advised(1, true, Vec::new());
    let h = run_advised(
        vec![
            ScriptedStep::Fixed(ScriptedReply::calls("", search()).consulting(ADVISOR)),
            ScriptedStep::calls("", search()),
            ScriptedStep::text(ANSWER),
        ],
        escalation,
    );
    let RunOutcome::Completed { checkpoint, .. } = h.start().await else {
        panic!("expected completion")
    };
    // Withdrawn, the reasoning bound to it dropped from then on.
    assert_eq!(
        advisors(&h),
        vec![(given(1), false), (None, true), (None, true)]
    );
    assert_eq!(checkpoint.escalation.advisor, AdvisorState::Withdrawn);
    assert_eq!(limited(&h), [Some("per_run".into())]);
}

#[tokio::test]
async fn a_paused_turn_keeps_its_advisor_past_the_run_s_escalations() {
    let (escalation, _) = advised(1, true, Vec::new());
    let h = run_advised(
        vec![
            // Consulted once, and once more still running.
            ScriptedStep::Fixed(ScriptedReply::text("").paused().consulting(ADVISOR)),
            ScriptedStep::Fixed(ScriptedReply::calls("", search()).consulting(ADVISOR)),
            ScriptedStep::text(ANSWER),
        ],
        escalation,
    );
    let RunOutcome::Completed { checkpoint, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert_eq!(
        advisors(&h),
        vec![(given(1), false), (given(1), false), (None, true)]
    );
    assert_eq!(checkpoint.escalation.calls, 2);
    assert_eq!(limited(&h), [Some("per_run".into())]);
}

#[tokio::test]
async fn past_the_month_s_budget_a_run_starts_without_its_advisor() {
    let (escalation, _) = advised(5, false, Vec::new());
    let h = run_advised(vec![ScriptedStep::text(ANSWER)], escalation);
    let RunOutcome::Completed { checkpoint, .. } = h.start().await else {
        panic!("expected completion")
    };
    // Never given: nothing to drop; nothing escalated, nothing journaled.
    assert_eq!(advisors(&h), vec![(None, false)]);
    assert_eq!(checkpoint.escalation.advisor, AdvisorState::Off);
    assert!(limited(&h).is_empty());
}

#[tokio::test]
async fn a_run_the_router_starts_on_the_escalation_model_has_no_advisor() {
    let stronger = Arc::new(ScriptedProvider::new(vec![ScriptedStep::text(ANSWER)]));
    let escalation = Escalation {
        provider: stronger.clone(),
        after_failure: false,
        router: Some(EscalationRoute::Escalation),
        advisor: Some(ADVISOR.into()),
        max_per_run: 5,
        budget: Arc::new(Budget(true)),
    };
    let h = run_advised(Vec::new(), escalation);
    let RunOutcome::Completed { checkpoint, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert_eq!(stronger.requests()[0].advisor, None);
    assert_eq!(checkpoint.escalation.advisor, AdvisorState::Off);
}

#[tokio::test]
async fn a_run_taken_up_once_its_company_withdrew_the_advisor_goes_on_without() {
    let (escalation, _) = advised(5, true, Vec::new());
    let h = run_advised(
        vec![ScriptedStep::calls(
            "",
            vec![call("planning_apply", json!({ "slots": 2 }))],
        )],
        escalation,
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause for validation")
    };
    // As another process reads it back.
    let stored = serde_json::to_value(&checkpoint).unwrap();
    let checkpoint = serde_json::from_value(stored).unwrap();
    // Taken up where the company's escalation was removed meanwhile.
    let taken_up = setup(vec![ScriptedStep::text(ANSWER)], Options::default());
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let RunOutcome::Completed { checkpoint, .. } = taken_up
        .runtime
        .resume(checkpoint, &decisions)
        .await
        .unwrap()
    else {
        panic!("expected completion")
    };
    let request = &taken_up.provider.requests()[0];
    assert_eq!(
        (request.advisor.clone(), request.drop_unbound_thinking),
        (None, true)
    );
    assert_eq!(checkpoint.escalation.advisor, AdvisorState::Withdrawn);
}
