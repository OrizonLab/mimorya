//! Model calls with bounded retries, and the accounting of what they cost.

use std::time::Duration;

use super::ports::{EnginePorts, RunEvent, UsageRecord};
use super::state::RunCheckpoint;
use crate::llm::{LlmError, LlmProvider, LlmRequest, LlmResponse, PriceTable};

/// Retries retryable provider errors with exponential backoff.
pub async fn complete_with_retry(
    provider: &dyn LlmProvider,
    request: &LlmRequest,
    max_retries: u32,
    delay: Duration,
) -> Result<LlmResponse, LlmError> {
    let mut attempt = 0;
    loop {
        match provider.complete(request).await {
            Ok(response) => return Ok(response),
            Err(error) if error.retryable() && attempt < max_retries => {
                tokio::time::sleep(delay * 2u32.pow(attempt)).await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Adds a model response to the run totals, records it for metering and
/// notifies listeners.
pub async fn record_model_usage(
    ports: &EnginePorts,
    prices: &PriceTable,
    state: &mut RunCheckpoint,
    response: &LlmResponse,
) -> anyhow::Result<()> {
    let cost = prices.cost_micros(&response.model, &response.usage);
    let usage = response.usage;
    let input_tokens = usage.input_total();
    state.usage.input_tokens += input_tokens;
    state.usage.output_tokens += usage.output_tokens;
    state.usage.cost_micros += cost.unwrap_or(0);
    ports
        .usage
        .record(UsageRecord {
            kind: "llm",
            provider: response.provider.clone(),
            model: response.model.clone(),
            input_tokens,
            output_tokens: usage.output_tokens,
            cost_micros: cost.unwrap_or(0),
        })
        .await?;
    ports
        .events
        .emit(RunEvent::Usage {
            input_tokens,
            output_tokens: usage.output_tokens,
            cost_micros: cost,
        })
        .await
}
