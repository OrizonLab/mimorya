//! The decision service (ADR-213) with simulated engines: the engine chosen
//! answers, low confidence passed on as said; on an error, a timeout or an
//! invalid answer, the next engine is asked; all failing, the outcome is
//! « failed », never an error. Each call that answered is counted, priced as
//! its model, an invalid one included; a request asked wrongly reaches no
//! engine. The main model as an engine: asked only while the operator's
//! supply allows (ADR-208).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use mimorya_agent::decision::{
    DecisionAnswer, DecisionCall, DecisionEngine, DecisionOutcome, DecisionQuestion,
    DecisionRecorder, DecisionRequest, DecisionService, DecisionState, EngineAnswer, EngineFailure,
    EngineKind, MainModelEngine,
};
use mimorya_agent::llm::{LlmProvider, LlmUsage, ModelRole, PriceTable};
use mimorya_agent::runtime::SupplyLimit;
use mimorya_agent::testing::{ScriptedProvider, ScriptedStep};
use mimorya_core::model_supply::SupplyStop;

#[derive(Default)]
struct Calls(Mutex<Vec<DecisionCall>>);

#[async_trait]
impl DecisionRecorder for Calls {
    async fn record(&self, call: DecisionCall) -> anyhow::Result<i64> {
        let charged = call.cost_micros.unwrap_or(0);
        self.0.lock().unwrap().push(call);
        Ok(charged)
    }
}

/// An engine that answers as told, after `delay`.
struct Simulated {
    kind: EngineKind,
    answer: Result<EngineAnswer, EngineFailure>,
    delay: Duration,
}

#[async_trait]
impl DecisionEngine for Simulated {
    fn kind(&self) -> EngineKind {
        self.kind
    }

    fn model(&self) -> &str {
        "jev-1.14.0"
    }

    async fn answer(&self, _: &DecisionRequest) -> Result<EngineAnswer, EngineFailure> {
        tokio::time::sleep(self.delay).await;
        self.answer.clone()
    }
}

fn request() -> DecisionRequest {
    DecisionRequest {
        usage: "test",
        state: DecisionState::new().field("message", "Bonjour", 100),
        questions: vec![DecisionQuestion::noul(
            "salutation",
            "Ce message est-il une salutation ?",
        )],
    }
}

fn said(yes: f64) -> EngineAnswer {
    EngineAnswer {
        provider: "typesafe".into(),
        model: "jev-1.14.0".into(),
        answers: BTreeMap::from([("salutation".into(), DecisionAnswer::Noul { yes })]),
        usage: LlmUsage {
            input_tokens: 1_000_000,
            output_tokens: 12,
            ..LlmUsage::default()
        },
    }
}

fn engine(answer: Result<EngineAnswer, EngineFailure>) -> Arc<dyn DecisionEngine> {
    Arc::new(Simulated {
        kind: EngineKind::Jev,
        answer,
        delay: Duration::ZERO,
    })
}

/// The main model, answering `yes` (as its strict JSON says it).
fn main_model(steps: Vec<ScriptedStep>) -> (Arc<dyn DecisionEngine>, Arc<ScriptedProvider>) {
    let provider = Arc::new(ScriptedProvider::new(steps));
    let engine = MainModelEngine::new(provider.clone() as Arc<dyn LlmProvider>, None);
    (Arc::new(engine), provider)
}

fn oui() -> ScriptedStep {
    ScriptedStep::text(r#"{"salutation": {"reponse": true, "certitude": "forte"}}"#)
}

const SOON: Duration = Duration::from_millis(200);

fn service(calls: &Arc<Calls>) -> DecisionService {
    DecisionService::new(calls.clone(), Arc::new(PriceTable::default()))
}

#[tokio::test]
async fn the_chosen_engine_answers_and_its_call_is_counted() {
    let calls = Arc::new(Calls::default());
    let (fallback, provider) = main_model(Vec::new());
    let decided = service(&calls)
        .then(engine(Ok(said(0.97))), SOON)
        .then(fallback, SOON)
        .decide(&request())
        .await;
    let DecisionOutcome::Decided(decided) = decided else {
        panic!("expected a decision")
    };
    assert_eq!(
        (decided.engine, decided.yes("salutation")),
        (EngineKind::Jev, Some(0.97))
    );
    assert!(provider.requests().is_empty());
    // A million input tokens of Jev: 0.042 USD.
    let counted = calls.0.lock().unwrap().clone();
    assert_eq!(counted.len(), 1);
    assert_eq!(
        (
            counted[0].usage,
            counted[0].provider.as_str(),
            counted[0].cost_micros
        ),
        ("test", "typesafe", Some(42_000))
    );
}

#[tokio::test]
async fn an_uncertain_answer_is_passed_on_as_said() {
    let calls = Arc::new(Calls::default());
    let decided = service(&calls)
        .then(engine(Ok(said(0.51))), SOON)
        .decide(&request())
        .await;
    let DecisionOutcome::Decided(decided) = decided else {
        panic!("expected a decision")
    };
    assert_eq!(decided.yes("salutation"), Some(0.51));
}

#[tokio::test]
async fn on_an_error_a_timeout_or_an_invalid_answer_the_main_model_answers() {
    let slow = Arc::new(Simulated {
        kind: EngineKind::Jev,
        answer: Ok(said(0.97)),
        delay: Duration::from_secs(5),
    });
    let mut invalid = said(0.97);
    invalid.answers = BTreeMap::from([("autre".into(), DecisionAnswer::Noul { yes: 0.9 })]);
    for (first, counted) in [
        (engine(Err(EngineFailure::Unavailable)), 1),
        (engine(Err(EngineFailure::Refused)), 1),
        (slow as Arc<dyn DecisionEngine>, 1),
        // Its call answered: counted, though its answer is not used.
        (engine(Ok(invalid)), 2),
    ] {
        let calls = Arc::new(Calls::default());
        let (fallback, provider) = main_model(vec![oui()]);
        let decided = service(&calls)
            .then(first, SOON)
            .then(fallback, SOON)
            .decide(&request())
            .await;
        let DecisionOutcome::Decided(decided) = decided else {
            panic!("expected the main model's decision")
        };
        assert_eq!(decided.engine, EngineKind::MainModel);
        assert_eq!(decided.yes("salutation"), Some(0.95));
        assert_eq!(provider.requests().len(), 1);
        assert_eq!(calls.0.lock().unwrap().len(), counted);
    }
}

#[tokio::test]
async fn every_engine_failing_the_outcome_is_failed_never_an_error() {
    let calls = Arc::new(Calls::default());
    let (fallback, _) = main_model(vec![ScriptedStep::text("Je pense que oui.")]);
    let outcome = service(&calls)
        .then(engine(Err(EngineFailure::Unavailable)), SOON)
        .then(fallback, SOON)
        .decide(&request())
        .await;
    assert_eq!(outcome, DecisionOutcome::Failed);
    // The main model answered something unreadable: its call counted.
    assert_eq!(calls.0.lock().unwrap().len(), 1);
    // Without any engine at all.
    let outcome = service(&calls).decide(&request()).await;
    assert_eq!(outcome, DecisionOutcome::Failed);
}

#[tokio::test]
async fn a_request_asked_wrongly_reaches_no_engine() {
    let calls = Arc::new(Calls::default());
    let (fallback, provider) = main_model(vec![oui()]);
    let mut wrong = request();
    wrong.questions.clear();
    let outcome = service(&calls).then(fallback, SOON).decide(&wrong).await;
    assert_eq!(outcome, DecisionOutcome::Failed);
    assert!(provider.requests().is_empty());
}

struct Spent;

#[async_trait]
impl SupplyLimit for Spent {
    async fn stopped(&self, role: ModelRole) -> anyhow::Result<Option<SupplyStop>> {
        assert_eq!(role, ModelRole::Decision);
        Ok(Some(SupplyStop::MonthlyCap))
    }
}

#[tokio::test]
async fn past_the_operator_s_supply_the_main_model_is_not_asked() {
    let provider = Arc::new(ScriptedProvider::new(vec![oui()]));
    let engine = MainModelEngine::new(
        provider.clone() as Arc<dyn LlmProvider>,
        Some(Arc::new(Spent)),
    );
    let answer = engine.answer(&request()).await;
    assert_eq!(answer, Err(EngineFailure::SupplySpent));
    assert!(provider.requests().is_empty());
}
