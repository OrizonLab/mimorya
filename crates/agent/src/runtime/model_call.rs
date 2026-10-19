//! Model calls with the tools the user may use at this step, bounded
//! retries, and the accounting of what they cost.

use std::sync::Arc;
use std::time::Duration;

use super::ports::{CurrentAccess, EnginePorts, RunEvent, UsageRecord};
use super::state::RunCheckpoint;
use super::types::{RunConfig, RuntimeDeps};
use crate::llm::{
    LlmError, LlmErrorKind, LlmProvider, LlmRequest, LlmResponse, PriceTable, TaskClass,
};
use crate::tools::to_tool_spec;

pub enum ModelTurn {
    Response(Box<LlmResponse>),
    Failed(String),
}

/// One model call with the tools visible to the subject at this step;
/// provider failures end the run.
pub async fn call_model(
    deps: &RuntimeDeps,
    config: &RunConfig,
    state: &mut RunCheckpoint,
    access: &CurrentAccess,
) -> anyhow::Result<ModelTurn> {
    let tools = deps
        .registry
        .visible_to(
            &access.engine,
            &access.subject,
            &config.actor,
            &config.autonomy,
        )
        .into_iter()
        .map(to_tool_spec)
        .collect();
    let events = deps.ports.events.clone();
    let request = LlmRequest {
        system: state.system.clone(),
        messages: state.messages.clone(),
        tools,
        max_output_tokens: None,
        effort: config.effort,
        on_text_delta: Some(Arc::new(move |text: &str| events.publish_delta(text))),
    };
    let provider = deps.router.provider_for(TaskClass::Agent);
    match complete_with_retry(
        provider.as_ref(),
        &request,
        deps.max_provider_retries,
        deps.retry_delay,
    )
    .await
    {
        Ok(response) => {
            state.usage.steps += 1;
            record_model_usage(&deps.ports, &deps.prices, state, &response).await?;
            Ok(ModelTurn::Response(Box::new(response)))
        }
        Err(error) if error.kind == LlmErrorKind::Aborted => {
            Ok(ModelTurn::Failed("Exécution annulée".into()))
        }
        Err(error) => Ok(ModelTurn::Failed(error.message)),
    }
}

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
