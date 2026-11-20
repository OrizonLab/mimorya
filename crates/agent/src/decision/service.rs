//! The decision service (ADR-213): the engines asked in turn, the one the
//! company chose first, then its main model, each within its time. An
//! answer is used only once checked against the questions; failing that,
//! the next engine is asked; all failing, the outcome is « failed », and
//! the usage applies the prudent value written in its code. It never
//! returns an error: a decision never blocks a run. Each call that
//! answered is counted (its tokens, priced as its model), and each decision
//! is journaled — usage, engine, model, answers, latency, outcome — never
//! the state it was asked about.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

use super::answer::{Decided, EngineKind, checked};
use super::engine::{DecisionEngine, EngineAnswer, EngineFailure};
use super::question::DecisionRequest;
use crate::llm::PriceTable;

/// A call that answered, as the consumption counts it.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionCall {
    pub usage: &'static str,
    pub engine: EngineKind,
    pub provider: String,
    pub model: String,
    /// Every input token, cached or not.
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// `None` when the model has no price.
    pub cost_micros: Option<i64>,
}

/// Where each call that answered is counted.
#[async_trait]
pub trait DecisionRecorder: Send + Sync {
    /// Records `call`; what it cost the company (ADR-212): its cost on the
    /// company's own key, the amount re-invoiced on the operator's.
    async fn record(&self, call: DecisionCall) -> anyhow::Result<i64>;
}

/// What a call that answered cost, as a run adds it to its totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counted {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Its cost, `None` when the model has no price.
    pub cost_micros: Option<i64>,
    /// What the company was charged; 0 when it could not be recorded.
    pub charged_micros: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecisionOutcome {
    Decided(Decided),
    /// No engine answered: the usage applies its prudent value.
    Failed,
}

struct Link {
    engine: Arc<dyn DecisionEngine>,
    timeout: Duration,
}

pub struct DecisionService {
    chain: Vec<Link>,
    recorder: Arc<dyn DecisionRecorder>,
    prices: Arc<PriceTable>,
}

impl DecisionService {
    pub fn new(recorder: Arc<dyn DecisionRecorder>, prices: Arc<PriceTable>) -> Self {
        Self {
            chain: Vec::new(),
            recorder,
            prices,
        }
    }

    /// Asked after those before, within `timeout`.
    pub fn then(mut self, engine: Arc<dyn DecisionEngine>, timeout: Duration) -> Self {
        self.chain.push(Link { engine, timeout });
        self
    }

    /// The engines asked, in turn.
    pub fn engines(&self) -> Vec<EngineKind> {
        self.chain.iter().map(|link| link.engine.kind()).collect()
    }

    pub async fn decide(&self, request: &DecisionRequest) -> DecisionOutcome {
        self.decide_counted(request).await.0
    }

    /// `decide`, with what each call that answered cost: what a run adds to
    /// its own totals.
    pub async fn decide_counted(
        &self,
        request: &DecisionRequest,
    ) -> (DecisionOutcome, Vec<Counted>) {
        self.decide_within(request, None).await
    }

    /// `decide_counted`, its engines asked within `budget` all together:
    /// each within what is left of it, so that a call that answered is
    /// always counted, whatever time is left (D3).
    pub async fn decide_within(
        &self,
        request: &DecisionRequest,
        budget: Option<Duration>,
    ) -> (DecisionOutcome, Vec<Counted>) {
        let deadline = budget.map(|budget| Instant::now() + budget);
        let mut counted = Vec::new();
        if let Some(problem) = request.problem() {
            tracing::warn!(
                usage = request.usage,
                problem,
                "Decision asked wrongly: failed"
            );
            return (DecisionOutcome::Failed, counted);
        }
        for link in &self.chain {
            let started = Instant::now();
            let left = deadline.map(|deadline| deadline.saturating_duration_since(started));
            if left.is_some_and(|left| left.is_zero()) {
                break;
            }
            let wait = left.map_or(link.timeout, |left| left.min(link.timeout));
            let asked = tokio::time::timeout(wait, link.engine.answer(request)).await;
            let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let answer = match asked {
                Err(_) => Err(EngineFailure::TimedOut),
                Ok(answer) => answer,
            };
            match answer {
                Ok(answer) => {
                    counted.push(self.count(request, link, &answer).await);
                    let model = answer.model.clone();
                    match checked(&request.questions, answer.answers) {
                        Some(answers) => {
                            let decided = Decided {
                                engine: link.engine.kind(),
                                model,
                                latency_ms,
                                answers,
                            };
                            journal_decided(request, &decided);
                            return (DecisionOutcome::Decided(decided), counted);
                        }
                        None => journal_failure(request, link, EngineFailure::Invalid, latency_ms),
                    }
                }
                Err(failure) => journal_failure(request, link, failure, latency_ms),
            }
        }
        tracing::warn!(
            target: "mimorya::decision",
            usage = request.usage,
            outcome = "failed",
            "No engine decided: the usage applies its prudent value"
        );
        (DecisionOutcome::Failed, counted)
    }

    /// The call counted, priced as its model; a failure to record it is
    /// journaled, never blocking.
    async fn count(
        &self,
        request: &DecisionRequest,
        link: &Link,
        answer: &EngineAnswer,
    ) -> Counted {
        let cost_micros = self
            .prices
            .cost_micros(&answer.model, &answer.usage)
            .or_else(|| self.prices.cost_micros(link.engine.model(), &answer.usage));
        let call = DecisionCall {
            usage: request.usage,
            engine: link.engine.kind(),
            provider: answer.provider.clone(),
            model: answer.model.clone(),
            input_tokens: answer.usage.input_total(),
            output_tokens: answer.usage.output_tokens,
            cost_micros,
        };
        let charged_micros = match self.recorder.record(call).await {
            Ok(charged) => charged,
            Err(error) => {
                tracing::warn!(error = %error, "Decision call not recorded");
                0
            }
        };
        Counted {
            input_tokens: answer.usage.input_total(),
            output_tokens: answer.usage.output_tokens,
            cost_micros,
            charged_micros,
        }
    }
}

fn journal_decided(request: &DecisionRequest, decided: &Decided) {
    let answers = serde_json::to_string(&decided.answers).unwrap_or_default();
    tracing::info!(
        target: "mimorya::decision",
        usage = request.usage,
        engine = decided.engine.as_str(),
        model = %decided.model,
        latency_ms = decided.latency_ms,
        outcome = "decided",
        answers = %answers,
        "Decision made"
    );
}

fn journal_failure(
    request: &DecisionRequest,
    link: &Link,
    failure: EngineFailure,
    latency_ms: u64,
) {
    tracing::warn!(
        target: "mimorya::decision",
        usage = request.usage,
        engine = link.engine.kind().as_str(),
        model = %link.engine.model(),
        latency_ms,
        outcome = failure.as_str(),
        "Decision engine failed: the next one is asked"
    );
}
