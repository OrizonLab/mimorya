//! Model usage: cached input tokens count once, in the run totals, the
//! metering record and the event listeners receive.

use mimorya_agent::llm::{LlmResponse, LlmUsage, PriceTable, StopReason};
use mimorya_agent::runtime::model_call::record_model_usage;
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
    };
    record_model_usage(
        &ports.ports(),
        &PriceTable::default(),
        &mut state,
        &response,
    )
    .await
    .unwrap();
    assert_eq!(state.usage.input_tokens, 55);
    assert_eq!(ports.usage_records()[0].input_tokens, 55);
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
