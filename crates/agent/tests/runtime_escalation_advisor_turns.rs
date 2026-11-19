//! Anthropic's advisor across turns and escalations (ADR-207, step 3):
//! Anthropic bounds the consultations of a turn, so that the run's bound
//! holds only while every escalation of the run is a consultation of the
//! current turn. The platform's correction opens a turn: once the advisor
//! was consulted, it is withdrawn there, kept otherwise. An escalated call
//! is an escalation Anthropic does not count: the escalation model is
//! called without it, and so is the main model taking over from it. A turn
//! paused while it is consulted goes on past the run's budget, once a run.
//! A consultation asked beside tools runs at the next request, which gives
//! the advisor whatever the bounds, the company's escalation or a skill's
//! switch; a stage handed over meanwhile asks no note. A run checkpointed
//! before the advisor existed goes on without it.

mod support;

use std::collections::HashMap;

use mimorya_agent::llm::{EscalationRoute, LlmMessage};
use mimorya_agent::runtime::{
    AdvisorState, ApprovalDecision, BudgetLimits, DEFAULT_BUDGET, NO_NOTE, RunCheckpoint,
    RunOutcome,
};
use mimorya_agent::testing::{ScriptedCall, ScriptedReply, ScriptedStep, SkillFixture, call};
use serde_json::{Value, json};
use support::advisor::{
    ADVISOR, advised, advisors, given, given_to, limited, run_advised, run_advised_with,
};
use support::{Options, setup};

const ANSWER: &str = "Synthèse : rien à signaler cette semaine.";

fn search() -> Vec<ScriptedCall> {
    vec![call("jobs_search", json!({ "query": "x" }))]
}

/// The main model searches, then cites an intervention the search did not
/// return, and keeps doing so through its two corrections.
fn failing_main_model() -> Vec<ScriptedStep> {
    vec![
        ScriptedStep::calls("", search()),
        ScriptedStep::text("INT-0999"),
        ScriptedStep::text("INT-0999 toujours"),
        ScriptedStep::text("INT-0999 encore"),
    ]
}

#[tokio::test]
async fn a_correction_after_a_consultation_opens_a_turn_it_is_withdrawn_from() {
    let (escalation, _) = advised(5, true, Vec::new());
    let h = run_advised(
        vec![
            ScriptedStep::Fixed(ScriptedReply::calls("", search()).consulting(ADVISOR)),
            ScriptedStep::text("INT-0999"),
            ScriptedStep::text(ANSWER),
        ],
        escalation,
    );
    let RunOutcome::Completed { checkpoint, .. } = h.start().await else {
        panic!("expected completion")
    };
    // Anthropic would allow five more in the new turn: four only remain.
    assert_eq!(
        advisors(&h),
        vec![(given(5), false), (given(5), false), (None, true)]
    );
    assert_eq!(checkpoint.escalation.advisor, AdvisorState::Withdrawn);
    // No bound reached: nothing journaled.
    assert!(limited(&h).is_empty());
}

#[tokio::test]
async fn a_correction_before_any_consultation_keeps_it() {
    let (escalation, _) = advised(5, true, Vec::new());
    let h = run_advised(
        vec![
            ScriptedStep::calls("", search()),
            ScriptedStep::text("INT-0999"),
            ScriptedStep::Fixed(ScriptedReply::text(ANSWER).consulting(ADVISOR)),
        ],
        escalation,
    );
    let RunOutcome::Completed { checkpoint, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert_eq!(advisors(&h), vec![(given(5), false); 3]);
    assert_eq!(checkpoint.escalation.turn_consultations, 1);
}

#[tokio::test]
async fn a_run_escalated_after_a_failed_review_calls_the_escalation_model_without_it() {
    let (mut escalation, stronger) = advised(5, true, vec![ScriptedStep::text(ANSWER)]);
    escalation.after_failure = true;
    let h = run_advised(failing_main_model(), escalation);
    let RunOutcome::Completed { answer, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert_eq!(answer, ANSWER);
    assert_eq!(advisors(&h), vec![(given(5), false); 4]);
    assert_eq!(given_to(stronger.requests()), vec![(None, true)]);
}

#[tokio::test]
async fn the_main_model_taking_over_from_a_failed_escalation_is_asked_the_same() {
    // The call and its two retries fail.
    let failing = (0..3)
        .map(|_| ScriptedStep::fails("Modèle surchargé"))
        .collect();
    let (mut escalation, stronger) = advised(5, true, failing);
    escalation.after_failure = true;
    let mut main = failing_main_model();
    main.push(ScriptedStep::text(ANSWER));
    let h = run_advised(main, escalation);
    assert_eq!(h.start().await.status(), "completed");
    assert_eq!(given_to(stronger.requests()), vec![(None, true); 3]);
    assert_eq!(advisors(&h).last(), Some(&(None, true)));
}

fn one_step() -> Options {
    Options {
        budget: Some(BudgetLimits {
            max_steps: 1,
            ..DEFAULT_BUDGET
        }),
        ..Options::default()
    }
}

#[tokio::test]
async fn a_paused_turn_goes_on_past_the_run_s_budget_once_a_run() {
    let paused = || ScriptedStep::Fixed(ScriptedReply::text("").paused());
    // Its one step spent by a paused turn: it goes on, and answers.
    let (escalation, _) = advised(5, true, Vec::new());
    let h = run_advised_with(
        vec![paused(), ScriptedStep::text(ANSWER)],
        escalation,
        one_step(),
    );
    let RunOutcome::Completed { checkpoint, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert_eq!(advisors(&h), vec![(given(5), false); 2]);
    assert!(checkpoint.escalation.paused_past_budget);

    // Paused again past it: the run stops there.
    let (escalation, _) = advised(5, true, Vec::new());
    let h = run_advised_with(vec![paused(), paused()], escalation, one_step());
    let RunOutcome::Failed { error, .. } = h.start().await else {
        panic!("expected the budget to stop the run")
    };
    assert!(error.contains("étapes"), "{error}");
    assert_eq!(h.provider.requests().len(), 2);
}

#[tokio::test]
async fn a_run_checkpointed_before_the_advisor_existed_goes_on_without_it() {
    let h = setup(
        vec![ScriptedStep::calls(
            "",
            vec![call("planning_apply", json!({ "slots": 2 }))],
        )],
        Options::default(),
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause for validation")
    };
    // As it was stored before: nothing said of the advisor.
    let mut stored = serde_json::to_value(&checkpoint).unwrap();
    if let Some(Value::Object(escalation)) = stored.get_mut("escalation") {
        escalation.remove("advisor");
    }
    let checkpoint: RunCheckpoint = serde_json::from_value(stored).unwrap();
    assert_eq!(checkpoint.escalation.advisor, AdvisorState::Undecided);

    // Taken up once its company has the advisor: never given mid-run.
    let (escalation, _) = advised(5, true, Vec::new());
    let taken_up = run_advised(vec![ScriptedStep::text(ANSWER)], escalation);
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let RunOutcome::Completed { checkpoint, .. } = taken_up
        .runtime
        .resume(checkpoint, &decisions)
        .await
        .unwrap()
    else {
        panic!("expected completion")
    };
    assert_eq!(advisors(&taken_up), vec![(None, false)]);
    assert_eq!(checkpoint.escalation.advisor, AdvisorState::Off);
}

/// The main model searches, calling its advisor beside: the consultation
/// runs at the next request.
fn searched_beside() -> ScriptedStep {
    ScriptedStep::Fixed(
        ScriptedReply::calls("", search())
            .consulting(ADVISOR)
            .pending(),
    )
}

#[tokio::test]
async fn a_consultation_asked_beside_tools_keeps_the_advisor_past_the_run_s_bound() {
    let (escalation, _) = advised(1, true, Vec::new());
    let h = run_advised(
        vec![
            searched_beside(),
            ScriptedStep::calls("", search()),
            ScriptedStep::text(ANSWER),
        ],
        escalation,
    );
    assert_eq!(h.start().await.status(), "completed");
    assert_eq!(
        advisors(&h),
        vec![(given(1), false), (given(1), false), (None, true)]
    );
    assert_eq!(limited(&h), [Some("per_run".into())]);
}

#[tokio::test]
async fn a_consultation_asked_beside_an_action_to_validate_is_given_its_advisor_at_resumption() {
    let (escalation, _) = advised(5, true, Vec::new());
    let apply = call("planning_apply", json!({ "slots": 2 }));
    let h = run_advised(
        vec![ScriptedStep::Fixed(
            ScriptedReply::calls("", vec![apply]).pending(),
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
    assert!(checkpoint.escalation.advisor_pending);
    // Taken up where the company's escalation was removed meanwhile: the
    // consultation still runs with the advisor it was asked with.
    let taken_up = setup(vec![ScriptedStep::text(ANSWER)], Options::default());
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let outcome = taken_up
        .runtime
        .resume(checkpoint, &decisions)
        .await
        .unwrap();
    assert_eq!(outcome.status(), "completed");
    assert_eq!(advisors(&taken_up), vec![(given(5), false)]);
}

#[tokio::test]
async fn a_skill_s_switch_waits_for_the_consultation_asked_beside_it() {
    let (mut escalation, stronger) = advised(5, true, vec![ScriptedStep::text(ANSWER)]);
    escalation.router = Some(EscalationRoute::Main);
    let skill = SkillFixture {
        name: "analyse-approfondie".into(),
        description: "Analyse approfondie des interventions".into(),
        content: "1. Croiser les interventions et leurs comptes rendus.".into(),
        quality_criteria: Vec::new(),
        model_escalation: true,
    };
    let load = call("skill_read", json!({ "name": "analyse-approfondie" }));
    let h = run_advised_with(
        vec![
            ScriptedStep::Fixed(ScriptedReply::calls("", vec![load]).pending()),
            ScriptedStep::calls("", search()),
        ],
        escalation,
        Options {
            platform: Some(Vec::new()),
            skills: vec![skill],
            ..Options::default()
        },
    );
    assert_eq!(h.start().await.status(), "completed");
    // The main model's call runs the consultation; the switch comes after.
    assert_eq!(advisors(&h), vec![(given(5), false), (given(5), false)]);
    assert_eq!(given_to(stronger.requests()), vec![(None, true)]);
}

#[tokio::test]
async fn a_stage_handed_over_while_a_consultation_is_to_run_asks_no_note() {
    let (escalation, _) = advised(5, true, Vec::new());
    let h = run_advised_with(
        vec![searched_beside(), searched_beside()],
        escalation,
        Options {
            handoff: true,
            ..one_step()
        },
    );
    let RunOutcome::Handoff { note, .. } = h.start().await else {
        panic!("expected the stage handed over")
    };
    // Past its budget once to let it run, then handed over without a call.
    assert_eq!(note, NO_NOTE);
    let requests = h.provider.requests();
    assert_eq!(requests.len(), 2);
    assert!(matches!(
        requests[1].last_message(),
        Some(LlmMessage::ToolResults { .. })
    ));
}
