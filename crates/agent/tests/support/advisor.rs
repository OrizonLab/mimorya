//! A company's escalation with Anthropic's advisor, as the runtime tests
//! give it (ADR-207, step 3): its model priced, its budget open or spent,
//! and what each call of a run was given.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{AdvisorTool, ModelPrice, PriceTable};
use mimorya_agent::runtime::{Escalation, EscalationBudget};
use mimorya_agent::testing::{RecordedRequest, ScriptedProvider, ScriptedStep};

use super::{Harness, Options, setup};

pub const ADVISOR: &str = "claude-opus-5-5";

pub struct Budget(pub bool);

#[async_trait]
impl EscalationBudget for Budget {
    async fn allows(&self) -> anyhow::Result<bool> {
        Ok(self.0)
    }
}

/// The advisor alone, `max_per_run` escalations a run, the month's budget
/// open or not; the escalation model answering `stronger`.
pub fn advised(
    max_per_run: u32,
    allows: bool,
    stronger: Vec<ScriptedStep>,
) -> (Escalation, Arc<ScriptedProvider>) {
    let provider = Arc::new(ScriptedProvider::new(stronger));
    let escalation = Escalation {
        provider: provider.clone(),
        after_failure: false,
        router: None,
        advisor: Some(ADVISOR.into()),
        max_per_run,
        budget: Arc::new(Budget(allows)),
    };
    (escalation, provider)
}

/// 4 and 20 USD a million tokens: a consultation (1,000 in, 400 out) costs
/// 0.012 USD.
pub fn prices() -> PriceTable {
    let price = ModelPrice {
        input_per_mtok: 4.0,
        output_per_mtok: 20.0,
        cache_read_per_mtok: None,
        cache_write_per_mtok: None,
        long_prompt: None,
    };
    PriceTable::new(HashMap::from([(ADVISOR.to_string(), price)]))
}

pub fn run_advised(main: Vec<ScriptedStep>, escalation: Escalation) -> Harness {
    run_advised_with(main, escalation, Options::default())
}

/// The same, as `options` say otherwise.
pub fn run_advised_with(
    main: Vec<ScriptedStep>,
    escalation: Escalation,
    options: Options,
) -> Harness {
    setup(
        main,
        Options {
            escalation: Some(escalation),
            prices: Some(prices()),
            ..options
        },
    )
}

/// The advisor a run with `max_per_run` escalations is given.
pub fn given(max_per_run: u32) -> Option<AdvisorTool> {
    Some(AdvisorTool {
        model: ADVISOR.into(),
        max_uses: max_per_run,
        cache: max_per_run >= 3,
    })
}

/// What each of `requests` was given: its advisor, and whether the
/// reasoning bound to a withdrawn one was dropped.
pub fn given_to(requests: Vec<RecordedRequest>) -> Vec<(Option<AdvisorTool>, bool)> {
    requests
        .into_iter()
        .map(|request| (request.advisor, request.drop_unbound_thinking))
        .collect()
}

/// The same, for the calls of the run's main model.
pub fn advisors(h: &Harness) -> Vec<(Option<AdvisorTool>, bool)> {
    given_to(h.provider.requests())
}

/// The reasons of the bounds journaled.
pub fn limited(h: &Harness) -> Vec<Option<String>> {
    h.ports
        .audit_entries()
        .into_iter()
        .filter(|entry| entry.action == "model_escalation.limited")
        .map(|entry| entry.reason)
        .collect()
}
