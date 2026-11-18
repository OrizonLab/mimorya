//! The router of the model escalation (ADR-207, step 2): while it is on, a
//! run starts on the model its company's rule names for its kind, or the
//! one a classification of its request by the main model names, in strict
//! JSON: any other answer, a failed classification, or the month's budget
//! spent keep the main model, and the classification is counted as such. A
//! skill loaded that asks for the escalation model switches the run, once,
//! and only the platform's `skill_read` may ask; nothing switches without
//! the router. A run taken up is not routed again.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{EscalationRoute, LlmMessage, ModelRole};
use mimorya_agent::runtime::{ApprovalDecision, Escalation, EscalationBudget, RunOutcome};
use mimorya_agent::testing::{ScriptedProvider, ScriptedStep, SkillFixture, call};
use serde_json::json;
use support::{Harness, Options, setup};

const ANSWER: &str = "Trois interventions cette semaine.";
const SKILL: &str = "analyse-approfondie";

struct Budget(bool);

#[async_trait]
impl EscalationBudget for Budget {
    async fn allows(&self) -> anyhow::Result<bool> {
        Ok(self.0)
    }
}

fn routed(
    route: Option<EscalationRoute>,
    steps: Vec<ScriptedStep>,
    allows: bool,
) -> (Escalation, Arc<ScriptedProvider>) {
    let provider = Arc::new(ScriptedProvider::new(steps));
    let escalation = Escalation {
        provider: provider.clone(),
        after_failure: false,
        router: route,
        max_per_run: 5,
        budget: Arc::new(Budget(allows)),
    };
    (escalation, provider)
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

fn started(h: &Harness) -> Vec<Option<String>> {
    h.ports
        .audit_entries()
        .into_iter()
        .filter(|entry| entry.action == "model_escalation.started")
        .map(|entry| entry.reason)
        .collect()
}

async fn answer(h: &Harness) -> String {
    match h.start().await {
        RunOutcome::Completed { answer, .. } => answer,
        _ => panic!("expected completion"),
    }
}

#[tokio::test]
async fn a_rule_starts_the_run_on_the_escalation_model() {
    let route = Some(EscalationRoute::Escalation);
    let (escalation, stronger) = routed(route, vec![ScriptedStep::text(ANSWER)], true);
    let h = run_with(Vec::new(), escalation);
    let RunOutcome::Completed {
        answer, checkpoint, ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, ANSWER);
    assert_eq!(stronger.requests().len(), 1);
    assert!(h.provider.requests().is_empty());
    assert_eq!(roles(&h), [ModelRole::Escalation]);
    assert_eq!(started(&h), [Some("rule".into())]);
    assert!(checkpoint.escalation.escalated);
}

#[tokio::test]
async fn a_rule_for_the_main_model_asks_nothing_more() {
    let route = Some(EscalationRoute::Main);
    let (escalation, stronger) = routed(route, Vec::new(), true);
    let h = run_with(vec![ScriptedStep::text(ANSWER)], escalation);
    assert_eq!(answer(&h).await, ANSWER);
    assert!(stronger.requests().is_empty());
    assert_eq!(roles(&h), [ModelRole::Main]);
    assert!(started(&h).is_empty());
}

#[tokio::test]
async fn a_classification_naming_the_escalation_model_starts_the_run_on_it() {
    let route = Some(EscalationRoute::Classification);
    let (escalation, stronger) = routed(route, vec![ScriptedStep::text(ANSWER)], true);
    let classified = ScriptedStep::text(r#"{"niveau": "escalade"}"#);
    let h = run_with(vec![classified], escalation);
    assert_eq!(answer(&h).await, ANSWER);
    assert_eq!(
        roles(&h),
        [ModelRole::Classification, ModelRole::Escalation]
    );
    assert_eq!(started(&h), [Some("classification".into())]);
    // The main model read the request alone, as a JSON string, without
    // tools.
    let requests = h.provider.requests();
    let asked = &requests[0];
    assert!(asked.tools.is_empty());
    assert!(asked.system.contains("JSON"), "{}", asked.system);
    let [LlmMessage::User { content }] = asked.messages.as_slice() else {
        panic!("expected the request alone")
    };
    assert!(
        content.contains(r#""Synthèse des interventions""#),
        "{content}"
    );
    assert_eq!(stronger.requests().len(), 1);
}

#[tokio::test]
async fn any_other_classification_keeps_the_main_model() {
    for classified in [
        r#"{"niveau": "principal"}"#,
        "escalade",
        "```json\n{\"niveau\": \"escalade\"}\n```",
        r#"{"niveau": "escalade", "raison": "difficile"}"#,
        r#"{"niveau": "Escalade"}"#,
        "",
    ] {
        let route = Some(EscalationRoute::Classification);
        let (escalation, stronger) = routed(route, Vec::new(), true);
        let main = vec![ScriptedStep::text(classified), ScriptedStep::text(ANSWER)];
        let h = run_with(main, escalation);
        assert_eq!(answer(&h).await, ANSWER, "{classified}");
        assert!(stronger.requests().is_empty(), "{classified}");
        assert_eq!(
            roles(&h),
            [ModelRole::Classification, ModelRole::Main],
            "{classified}"
        );
        assert!(started(&h).is_empty(), "{classified}");
    }
}

#[tokio::test]
async fn a_failed_classification_keeps_the_main_model() {
    let route = Some(EscalationRoute::Classification);
    let (escalation, stronger) = routed(route, Vec::new(), true);
    let main = vec![ScriptedStep::fails("surchargé"), ScriptedStep::text(ANSWER)];
    let h = run_with(main, escalation);
    assert_eq!(answer(&h).await, ANSWER);
    assert!(stronger.requests().is_empty());
    assert_eq!(roles(&h), [ModelRole::Main]);
}

#[tokio::test]
async fn past_the_month_s_budget_no_classification_is_asked() {
    let route = Some(EscalationRoute::Classification);
    let (escalation, stronger) = routed(route, Vec::new(), false);
    let h = run_with(vec![ScriptedStep::text(ANSWER)], escalation);
    assert_eq!(answer(&h).await, ANSWER);
    assert!(stronger.requests().is_empty());
    assert_eq!(roles(&h), [ModelRole::Main]);
    let limited: Vec<_> = h
        .ports
        .audit_entries()
        .into_iter()
        .filter(|entry| entry.action == "model_escalation.limited")
        .map(|entry| entry.reason)
        .collect();
    assert_eq!(limited, [Some("monthly_budget".into())]);
}

/// The company's skill that asks for the escalation model, or not.
fn skill_run(asks: bool, route: Option<EscalationRoute>) -> (Harness, Arc<ScriptedProvider>) {
    let (escalation, stronger) = routed(route, vec![ScriptedStep::text(ANSWER)], true);
    let skill = SkillFixture {
        name: SKILL.into(),
        description: "Analyse approfondie des interventions".into(),
        content: "1. Croiser les interventions et leurs comptes rendus.".into(),
        quality_criteria: Vec::new(),
        model_escalation: asks,
    };
    let load = ScriptedStep::calls("", vec![call("skill_read", json!({ "name": SKILL }))]);
    let h = setup(
        vec![load, ScriptedStep::text(ANSWER)],
        Options {
            platform: Some(Vec::new()),
            skills: vec![skill],
            escalation: Some(escalation),
            ..Options::default()
        },
    );
    (h, stronger)
}

#[tokio::test]
async fn a_skill_that_asks_for_the_escalation_model_switches_the_run_once_loaded() {
    let (h, stronger) = skill_run(true, Some(EscalationRoute::Main));
    let RunOutcome::Completed {
        answer, checkpoint, ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, ANSWER);
    assert_eq!(stronger.requests().len(), 1);
    assert_eq!(roles(&h), [ModelRole::Main, ModelRole::Escalation]);
    assert_eq!(started(&h), [Some("skill".into())]);
    assert!(checkpoint.escalation.escalated && checkpoint.escalation.skill_heeded);
}

#[tokio::test]
async fn a_skill_switches_nothing_without_the_router_nor_when_it_does_not_ask() {
    for (asks, route) in [(true, None), (false, Some(EscalationRoute::Main))] {
        let (h, stronger) = skill_run(asks, route);
        assert_eq!(answer(&h).await, ANSWER);
        assert!(stronger.requests().is_empty(), "{asks} {route:?}");
        assert_eq!(roles(&h), [ModelRole::Main, ModelRole::Main]);
        assert!(started(&h).is_empty());
    }
}

#[tokio::test]
async fn only_the_platform_s_skill_read_may_ask_for_the_escalation_model() {
    let route = Some(EscalationRoute::Main);
    let (escalation, stronger) = routed(route, Vec::new(), true);
    // `doc_read` claims to ask for it.
    let read = ScriptedStep::calls("", vec![call("doc_read", json!({}))]);
    let h = run_with(vec![read, ScriptedStep::text(ANSWER)], escalation);
    assert_eq!(answer(&h).await, ANSWER);
    assert!(stronger.requests().is_empty());
    assert_eq!(roles(&h), [ModelRole::Main, ModelRole::Main]);
}

#[tokio::test]
async fn a_run_taken_up_after_a_validation_is_not_routed_again() {
    let route = Some(EscalationRoute::Classification);
    let (escalation, stronger) = routed(
        route,
        vec![
            ScriptedStep::calls("", vec![call("planning_apply", json!({ "slots": 2 }))]),
            ScriptedStep::text("Deux créneaux réservés."),
        ],
        true,
    );
    let classified = ScriptedStep::text(r#"{"niveau": "escalade"}"#);
    let h = run_with(vec![classified], escalation);
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause for validation")
    };
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let outcome = h.runtime.resume(checkpoint, &decisions).await.unwrap();
    assert_eq!(outcome.status(), "completed");
    // Classified once, at the start.
    assert_eq!(h.provider.requests().len(), 1);
    assert_eq!(stronger.requests().len(), 2);
    assert_eq!(started(&h), [Some("classification".into())]);
}
