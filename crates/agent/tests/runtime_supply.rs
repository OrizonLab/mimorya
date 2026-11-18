//! The operator's supply of a run's model (ADR-208, step 1): past it, a run
//! makes no call to the model and says why; one under way stops before its
//! next call, what it did kept; an answer is left unjudged, a request not
//! classified; a sub-agent is held to it as its parent is. The stop is
//! journaled once a run, without content.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use mimorya_agent::llm::{EscalationRoute, ModelRole};
use mimorya_agent::runtime::{Escalation, EscalationBudget, RunOutcome, SupplyLimit};
use mimorya_agent::testing::{ScriptedCall, ScriptedProvider, ScriptedStep, call};
use mimorya_core::model_supply::SupplyStop;
use serde_json::json;
use support::{Harness, Options, setup};

/// A supply that allows `checks` checks, then stops `roles`.
struct Supply {
    checks: AtomicU32,
    roles: Vec<ModelRole>,
    stop: SupplyStop,
}

impl Supply {
    fn after(checks: u32, stop: SupplyStop) -> Arc<Self> {
        Arc::new(Self {
            checks: AtomicU32::new(checks),
            roles: ModelRole::ALL.to_vec(),
            stop,
        })
    }
}

#[async_trait]
impl SupplyLimit for Supply {
    async fn stopped(&self, role: ModelRole) -> anyhow::Result<Option<SupplyStop>> {
        let left = self.checks.load(Ordering::SeqCst);
        if left > 0 {
            self.checks.store(left - 1, Ordering::SeqCst);
            return Ok(None);
        }
        Ok(self.roles.contains(&role).then_some(self.stop))
    }
}

fn limited(h: &Harness) -> Vec<Option<String>> {
    h.ports
        .audit_entries()
        .into_iter()
        .filter(|entry| entry.action == "model_supply.limited")
        .map(|entry| entry.reason)
        .collect()
}

fn failed_with(outcome: RunOutcome) -> String {
    match outcome {
        RunOutcome::Failed { error, .. } => error,
        other => panic!("expected the run to stop, got {}", other.status()),
    }
}

fn search() -> ScriptedCall {
    call("jobs_search", json!({ "query": "x" }))
}

#[tokio::test]
async fn past_the_supply_a_run_makes_no_call_and_says_why() {
    let supply = Supply::after(0, SupplyStop::CreditSpent);
    let h = setup(
        vec![ScriptedStep::text("jamais")],
        Options {
            supply: Some(supply),
            ..Options::default()
        },
    );
    assert_eq!(
        failed_with(h.start().await),
        SupplyStop::CreditSpent.message()
    );
    assert!(h.provider.requests().is_empty());
    assert_eq!(limited(&h), [Some("credit_spent".into())]);
}

#[tokio::test]
async fn a_run_stops_before_its_next_call_once_the_supply_is_spent() {
    let supply = Supply::after(1, SupplyStop::MonthlyCap);
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![search()]),
            ScriptedStep::text("jamais"),
        ],
        Options {
            supply: Some(supply),
            ..Options::default()
        },
    );
    assert_eq!(
        failed_with(h.start().await),
        SupplyStop::MonthlyCap.message()
    );
    // The first call made, its search done; no second call.
    assert_eq!(h.provider.requests().len(), 1);
    let searches = h
        .ports
        .tool_records()
        .iter()
        .filter(|record| record.tool_name == "jobs_search" && record.status == "succeeded")
        .count();
    assert_eq!(searches, 1);
    assert_eq!(limited(&h), [Some("monthly_cap".into())]);
}

#[tokio::test]
async fn past_the_supply_an_answer_is_left_unjudged() {
    let supply = Supply::after(1, SupplyStop::MonthlyCap);
    let h = setup(
        vec![ScriptedStep::text("Trois interventions cette semaine.")],
        Options {
            supply: Some(supply),
            criteria: vec!["Dire combien.".into()],
            judge: Some(vec![ScriptedStep::text("jamais")]),
            llm_judge: Some(true),
            ..Options::default()
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
    assert_eq!(answer, "Trois interventions cette semaine.");
    assert!(!verification.judged);
    assert!(h.judge.as_ref().unwrap().requests().is_empty());
    assert_eq!(limited(&h), [Some("monthly_cap".into())]);
}

struct Unbounded;

#[async_trait]
impl EscalationBudget for Unbounded {
    async fn allows(&self) -> anyhow::Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn past_the_supply_a_request_is_not_classified() {
    // The classification's role alone is held back.
    let supply = Arc::new(Supply {
        checks: AtomicU32::new(0),
        roles: vec![ModelRole::Classification],
        stop: SupplyStop::MonthlyCap,
    });
    let stronger = Arc::new(ScriptedProvider::new(Vec::new()));
    let escalation = Escalation {
        provider: stronger.clone(),
        after_failure: false,
        router: Some(EscalationRoute::Classification),
        max_per_run: 5,
        budget: Arc::new(Unbounded),
    };
    let h = setup(
        vec![ScriptedStep::text("Trois interventions cette semaine.")],
        Options {
            supply: Some(supply),
            escalation: Some(escalation),
            ..Options::default()
        },
    );
    assert_eq!(h.start().await.status(), "completed");
    // The run's own call alone: no classification, no escalation.
    assert_eq!(h.provider.requests().len(), 1);
    assert!(stronger.requests().is_empty());
}

#[tokio::test]
async fn a_sub_agent_is_held_to_the_supply_as_its_parent_is() {
    let supply = Supply::after(1, SupplyStop::CreditSpent);
    let delegate = call(
        "delegate_task",
        json!({ "profile": "research", "task": "Lister les interventions" }),
    );
    let h = setup(
        vec![
            ScriptedStep::calls("Je confie la recherche.", vec![delegate]),
            ScriptedStep::text("jamais"),
        ],
        Options {
            supply: Some(supply),
            sub_agents: true,
            ..Options::default()
        },
    );
    assert_eq!(
        failed_with(h.start().await),
        SupplyStop::CreditSpent.message()
    );
    // The parent's first call alone: the child made none, nor its parent
    // after it.
    assert_eq!(h.provider.requests().len(), 1);
}
