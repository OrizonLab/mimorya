//! The router's classification asked of the decision service (ADR-213, D1),
//! with simulated engines: active, the escalation model named, or the main
//! model named with a confidence below the threshold, starts the run on the
//! escalation model, journaled as a decision; the main model named with
//! confidence keeps it, the classification not asked. An engine failing —
//! an error, a timeout, an answer off its question — leaves the decision to
//! the main model's classification, as before. Observed, it changes
//! nothing. A decision never lifts a bound: the month's budget spent, it is
//! not even asked. Each call that answered is counted in the run.

mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use mimorya_agent::decision::{
    DecisionAnswer, DecisionCall, DecisionEngine, DecisionMode, DecisionRecorder, DecisionRequest,
    DecisionService, DecisionUsage, EngineAnswer, EngineFailure, EngineKind, RunDecisions,
    UsagePolicy,
};
use mimorya_agent::llm::{EscalationRoute, LlmUsage, ModelRole, PriceTable};
use mimorya_agent::runtime::{Escalation, EscalationBudget, RunOutcome};
use mimorya_agent::testing::{ScriptedProvider, ScriptedStep};
use support::{Harness, Options, setup};

const ANSWER: &str = "Trois interventions cette semaine.";

struct Budget(bool);

#[async_trait]
impl EscalationBudget for Budget {
    async fn allows(&self) -> anyhow::Result<bool> {
        Ok(self.0)
    }
}

#[derive(Default)]
struct Calls(Mutex<Vec<DecisionCall>>);

#[async_trait]
impl DecisionRecorder for Calls {
    async fn record(&self, call: DecisionCall) -> anyhow::Result<i64> {
        self.0.lock().unwrap().push(call);
        Ok(7)
    }
}

/// An engine that answers as told, after `delay`.
struct Simulated {
    answer: Result<EngineAnswer, EngineFailure>,
    delay: Duration,
}

#[async_trait]
impl DecisionEngine for Simulated {
    fn kind(&self) -> EngineKind {
        EngineKind::Jev
    }

    fn model(&self) -> &str {
        "jev-1.14.0"
    }

    async fn answer(&self, _: &DecisionRequest) -> Result<EngineAnswer, EngineFailure> {
        tokio::time::sleep(self.delay).await;
        self.answer.clone()
    }
}

/// `choice` chosen with `confidence`, for the question `question`.
fn chose(question: &str, choice: &str, confidence: f64) -> EngineAnswer {
    let other = if choice == "escalade" {
        "principal"
    } else {
        "escalade"
    };
    let answer = DecisionAnswer::Choice {
        choice: choice.into(),
        confidence,
        probabilities: BTreeMap::from([
            (choice.into(), confidence),
            (other.into(), 1.0 - confidence),
        ]),
    };
    EngineAnswer {
        provider: "typesafe".into(),
        model: "jev-1.14.0".into(),
        answers: BTreeMap::from([(question.into(), answer)]),
        usage: LlmUsage {
            input_tokens: 120,
            output_tokens: 0,
            ..LlmUsage::default()
        },
    }
}

struct Setup {
    answer: Result<EngineAnswer, EngineFailure>,
    delay: Duration,
    mode: DecisionMode,
    threshold: f64,
    /// Whether the month's escalation budget allows a call.
    allows: bool,
}

impl Setup {
    fn answering(answer: EngineAnswer) -> Self {
        Self {
            answer: Ok(answer),
            delay: Duration::ZERO,
            mode: DecisionMode::Active,
            threshold: 0.5,
            allows: true,
        }
    }
}

/// A run routed by classification, the decision service set as `setup`
/// says, the main model scripted with `main`.
fn run(setup_: Setup, main: Vec<ScriptedStep>) -> (Harness, Arc<ScriptedProvider>, Arc<Calls>) {
    let stronger = Arc::new(ScriptedProvider::new(vec![ScriptedStep::text(ANSWER)]));
    let escalation = Escalation {
        provider: stronger.clone(),
        after_failure: false,
        router: Some(EscalationRoute::Classification),
        advisor: None,
        max_per_run: 5,
        budget: Arc::new(Budget(setup_.allows)),
    };
    let calls = Arc::new(Calls::default());
    let engine = Arc::new(Simulated {
        answer: setup_.answer,
        delay: setup_.delay,
    });
    let service = DecisionService::new(calls.clone(), Arc::new(PriceTable::default()))
        .then(engine, Duration::from_millis(100));
    let policy = UsagePolicy {
        mode: setup_.mode,
        threshold: setup_.threshold,
    };
    let decisions = RunDecisions::new(
        Arc::new(service),
        vec![(DecisionUsage::ModelRouting, policy)],
    );
    let h = setup(
        main,
        Options {
            escalation: Some(escalation),
            decisions: Some(Arc::new(decisions)),
            ..Options::default()
        },
    );
    (h, stronger, calls)
}

fn started(h: &Harness) -> Vec<Option<String>> {
    h.ports
        .audit_entries()
        .into_iter()
        .filter(|entry| entry.action == "model_escalation.started")
        .map(|entry| entry.reason)
        .collect()
}

fn roles(h: &Harness) -> Vec<ModelRole> {
    h.ports.usage_records().iter().map(|r| r.role).collect()
}

async fn completed(h: &Harness) -> (String, u64) {
    match h.start().await {
        RunOutcome::Completed {
            answer, checkpoint, ..
        } => (answer, checkpoint.usage.input_tokens),
        _ => panic!("expected completion"),
    }
}

#[tokio::test]
async fn the_escalation_model_named_starts_the_run_on_it_and_the_decision_is_counted() {
    let (h, stronger, calls) = run(
        Setup::answering(chose("niveau", "escalade", 0.9)),
        Vec::new(),
    );
    let (answer, input_tokens) = completed(&h).await;
    assert_eq!(answer, ANSWER);
    // Neither classified nor answered by the main model.
    assert!(h.provider.requests().is_empty());
    assert_eq!(stronger.requests().len(), 1);
    assert_eq!(roles(&h), [ModelRole::Escalation]);
    assert_eq!(started(&h), [Some("decision".into())]);
    // The decision's call counted, and in the run's totals.
    assert_eq!(calls.0.lock().unwrap().len(), 1);
    assert!(input_tokens >= 120, "{input_tokens}");
}

#[tokio::test]
async fn the_main_model_named_with_confidence_keeps_it_unclassified() {
    let main = vec![ScriptedStep::text(ANSWER)];
    let (h, stronger, _) = run(Setup::answering(chose("niveau", "principal", 0.9)), main);
    assert_eq!(completed(&h).await.0, ANSWER);
    assert_eq!(h.provider.requests().len(), 1);
    assert!(stronger.requests().is_empty());
    assert_eq!(roles(&h), [ModelRole::Main]);
    assert!(started(&h).is_empty());
}

#[tokio::test]
async fn too_little_confidence_in_the_main_model_escalates() {
    let mut low = Setup::answering(chose("niveau", "principal", 0.6));
    low.threshold = 0.7;
    let (h, stronger, _) = run(low, Vec::new());
    assert_eq!(completed(&h).await.0, ANSWER);
    assert_eq!(stronger.requests().len(), 1);
    assert_eq!(started(&h), [Some("decision".into())]);
}

#[tokio::test]
async fn an_engine_failing_leaves_it_to_the_classification() {
    let failing = [
        Setup {
            answer: Err(EngineFailure::Unavailable),
            ..Setup::answering(chose("niveau", "escalade", 0.9))
        },
        Setup {
            delay: Duration::from_secs(5),
            ..Setup::answering(chose("niveau", "escalade", 0.9))
        },
        // An answer to another question.
        Setup::answering(chose("autre", "escalade", 0.9)),
    ];
    for setup_ in failing {
        let classified = ScriptedStep::text(r#"{"niveau": "principal"}"#);
        let (h, stronger, _) = run(setup_, vec![classified, ScriptedStep::text(ANSWER)]);
        assert_eq!(completed(&h).await.0, ANSWER);
        // The main model classified, then answered.
        assert_eq!(roles(&h), [ModelRole::Classification, ModelRole::Main]);
        assert!(stronger.requests().is_empty());
        assert!(started(&h).is_empty());
    }
}

#[tokio::test]
async fn observed_the_decision_changes_nothing() {
    let mut observed = Setup::answering(chose("niveau", "escalade", 0.99));
    observed.mode = DecisionMode::Observe;
    let classified = ScriptedStep::text(r#"{"niveau": "principal"}"#);
    let (h, stronger, calls) = run(observed, vec![classified, ScriptedStep::text(ANSWER)]);
    assert_eq!(completed(&h).await.0, ANSWER);
    // Asked and counted, yet the classification routed the run.
    assert_eq!(calls.0.lock().unwrap().len(), 1);
    assert_eq!(roles(&h), [ModelRole::Classification, ModelRole::Main]);
    assert!(stronger.requests().is_empty());
    assert!(started(&h).is_empty());
}

#[tokio::test]
async fn a_decision_never_lifts_the_escalation_s_bounds() {
    // The month's budget spent: nothing is asked, the run keeps its main model.
    let mut spent = Setup::answering(chose("niveau", "escalade", 0.99));
    spent.allows = false;
    let (h, stronger, calls) = run(spent, vec![ScriptedStep::text(ANSWER)]);
    assert_eq!(completed(&h).await.0, ANSWER);
    assert!(calls.0.lock().unwrap().is_empty());
    assert!(stronger.requests().is_empty());
    assert_eq!(roles(&h), [ModelRole::Main]);
    assert!(started(&h).is_empty());
    // Off: nothing is asked, the classification routes the run.
    let mut off = Setup::answering(chose("niveau", "escalade", 0.99));
    off.mode = DecisionMode::Off;
    let classified = ScriptedStep::text(r#"{"niveau": "principal"}"#);
    let (h, _, calls) = run(off, vec![classified, ScriptedStep::text(ANSWER)]);
    assert_eq!(completed(&h).await.0, ANSWER);
    assert!(calls.0.lock().unwrap().is_empty());
    assert_eq!(roles(&h), [ModelRole::Classification, ModelRole::Main]);
}
