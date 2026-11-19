//! Model usage: cached input tokens count once, in the run totals, the
//! metering record and the event listeners receive; a call's cost and what
//! the company was charged for it are counted apart (ADR-212).

use std::collections::HashMap;

use mimorya_agent::llm::{LlmResponse, LlmUsage, ModelPrice, ModelRole, PriceTable, StopReason};
use mimorya_agent::runtime::model_call::{ModelCall, record_model_usage};
use mimorya_agent::runtime::{RunCheckpoint, RunEvent};
use mimorya_agent::testing::InMemoryPorts;
use mimorya_core::demo::user;
use uuid::Uuid;

#[tokio::test]
async fn cached_input_tokens_count_once_everywhere() {
    let ports = InMemoryPorts::new(user::LUCAS, Uuid::new_v4());
    let mut state = RunCheckpoint::new("Consignes".into(), Vec::new());
    let response = LlmResponse {
        text: "Bonjour.".into(),
        tool_calls: Vec::new(),
        stop_reason: StopReason::EndTurn,
        usage: LlmUsage {
            input_tokens: 40,
            output_tokens: 7,
            cache_read_tokens: 10,
            cache_write_tokens: 5,
        },
        provider: "scripted".into(),
        model: "scripted-model".into(),
        native: None,
        consultations: Vec::new(),
        advisor_pending: false,
    };
    record_model_usage(
        &ports.ports(),
        &PriceTable::default(),
        &mut state,
        &response,
        ModelCall {
            role: ModelRole::Main,
            by_runs_model: true,
            requested: "scripted-model",
        },
    )
    .await
    .unwrap();
    assert_eq!(state.usage.input_tokens, 55);
    assert_eq!(ports.usage_records()[0].input_tokens, 55);
    assert_eq!(ports.usage_records()[0].role, ModelRole::Main);
    let emitted = ports.emitted();
    let Some(RunEvent::Usage {
        input_tokens,
        output_tokens,
        ..
    }) = emitted.last()
    else {
        panic!("expected a usage event, got {emitted:?}");
    };
    assert_eq!((*input_tokens, *output_tokens), (55, 7));
}

#[tokio::test]
async fn what_a_call_cost_and_what_the_company_was_charged_are_counted_apart() {
    let ports = InMemoryPorts::new(user::LUCAS, Uuid::new_v4());
    // Charged 10 % more, as on the operator's key.
    ports.charge_at(1_000);
    let price = ModelPrice {
        input_per_mtok: 1500.0,
        output_per_mtok: 2500.0,
        cache_read_per_mtok: None,
        cache_write_per_mtok: None,
        long_prompt: None,
    };
    let prices = PriceTable::new(HashMap::from([("scripted-model".to_string(), price)]));
    let mut state = RunCheckpoint::new("Consignes".into(), Vec::new());
    let response = LlmResponse {
        text: "Bonjour.".into(),
        tool_calls: Vec::new(),
        stop_reason: StopReason::EndTurn,
        usage: LlmUsage {
            input_tokens: 100,
            output_tokens: 20,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        },
        provider: "scripted".into(),
        model: "scripted-model".into(),
        native: None,
        consultations: Vec::new(),
        advisor_pending: false,
    };
    let call = || ModelCall {
        role: ModelRole::Main,
        by_runs_model: true,
        requested: "scripted-model",
    };
    for _ in 0..2 {
        record_model_usage(&ports.ports(), &prices, &mut state, &response, call())
            .await
            .unwrap();
    }
    assert_eq!(state.usage.cost_micros, 400_000);
    assert_eq!(state.usage.charged(), 440_000);
    let emitted = ports.emitted();
    let Some(RunEvent::Usage { cost_micros, .. }) = emitted.last() else {
        panic!("expected a usage event, got {emitted:?}");
    };
    assert_eq!(*cost_micros, Some(220_000));
}
