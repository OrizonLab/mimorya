//! A run's Anthropic advisor (ADR-207, step 3): the escalation model, which
//! the main model consults while it answers. It is decided at the run's
//! first call: given while the company has it and the run starts on its
//! main model within its bounds. It then goes unchanged to every call, the
//! escalation model's included, since changing it would edit the tools the
//! run's reasoning is bound to. It is withdrawn once for good when a bound
//! is reached (journaled once, as an escalation's) or the company's
//! escalation changed, the reasoning bound to it then dropped rather than
//! refused; never while a consultation is still running. Each consultation
//! is an escalation: counted in the run's, recorded and priced as its model.

use super::escalation::{bound_reached, limited};
use super::ports::{EnginePorts, RunEvent, UsageRecord};
use super::state::{AdvisorState, RunCheckpoint};
use super::types::RuntimeDeps;
use crate::llm::{AdvisorTool, LlmResponse, ModelRole, PriceTable, StopReason, advisor_tool};

/// The advisor of a call, and whether the reasoning bound to it before it
/// was withdrawn is dropped.
#[derive(Debug, Default)]
pub(super) struct AdvisorCall {
    pub tool: Option<AdvisorTool>,
    pub drop_unbound_thinking: bool,
}

/// The advisor of the run's next call, its model already chosen.
pub(super) async fn advisor_for_call(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
) -> anyhow::Result<AdvisorCall> {
    if state.escalation.advisor == AdvisorState::Undecided {
        state.escalation.advisor = decided(deps, state).await?;
    }
    let AdvisorState::Given(tool) = state.escalation.advisor.clone() else {
        return Ok(AdvisorCall {
            tool: None,
            drop_unbound_thinking: state.escalation.advisor == AdvisorState::Withdrawn,
        });
    };
    // A paused turn goes on with the tool it paused with.
    if !state.escalation.advisor_pending && !still_given(deps, state, &tool).await? {
        state.escalation.advisor = AdvisorState::Withdrawn;
        return Ok(AdvisorCall {
            tool: None,
            drop_unbound_thinking: true,
        });
    }
    Ok(AdvisorCall {
        tool: Some(tool),
        drop_unbound_thinking: false,
    })
}

/// At the run's first call: given while the company has it and the run
/// starts on its main model, within its bounds.
async fn decided(deps: &RuntimeDeps, state: &mut RunCheckpoint) -> anyhow::Result<AdvisorState> {
    let Some(escalation) = deps.escalation.as_ref() else {
        return Ok(AdvisorState::Off);
    };
    let Some(model) = escalation.advisor.as_deref() else {
        return Ok(AdvisorState::Off);
    };
    if state.escalation.escalated {
        return Ok(AdvisorState::Off);
    }
    if let Some(bound) = bound_reached(escalation, state).await? {
        limited(deps, state, bound).await?;
        return Ok(AdvisorState::Off);
    }
    Ok(AdvisorState::Given(advisor_tool(
        model,
        escalation.max_per_run,
    )))
}

/// Whether the advisor given stays: the company still has it, the same, and
/// the run's bounds allow another consultation.
async fn still_given(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
    tool: &AdvisorTool,
) -> anyhow::Result<bool> {
    let escalation = deps
        .escalation
        .as_ref()
        .filter(|e| e.advisor.as_deref() == Some(tool.model.as_str()));
    let Some(escalation) = escalation else {
        tracing::warn!("The company's advisor changed during the run: withdrawn");
        return Ok(false);
    };
    match bound_reached(escalation, state).await? {
        Some(bound) => {
            limited(deps, state, bound).await?;
            Ok(false)
        }
        None => Ok(true),
    }
}

/// After a call: its consultations counted in the run's escalations, and
/// whether one is still running.
pub(super) fn after_call(state: &mut RunCheckpoint, response: &LlmResponse) {
    let consulted = u32::try_from(response.consultations.len()).unwrap_or(u32::MAX);
    state.escalation.calls = state.escalation.calls.saturating_add(consulted);
    state.escalation.advisor_pending = response.stop_reason == StopReason::PauseTurn
        && matches!(state.escalation.advisor, AdvisorState::Given(_));
}

/// Records each consultation of a call as the advisor's, priced as its
/// model (else as the advisor asked for: never for nothing), in the run's
/// totals and its listeners'.
pub(super) async fn record_consultations(
    ports: &EnginePorts,
    prices: &PriceTable,
    state: &mut RunCheckpoint,
    response: &LlmResponse,
    asked: Option<&str>,
) -> anyhow::Result<()> {
    for consultation in &response.consultations {
        let usage = &consultation.usage;
        let cost = prices
            .cost_micros(&consultation.model, usage)
            .or_else(|| asked.and_then(|model| prices.cost_micros(model, usage)));
        let input_tokens = usage.input_total();
        let charged = ports
            .usage
            .record(UsageRecord {
                kind: "llm",
                role: ModelRole::Advisor,
                by_runs_model: false,
                provider: response.provider.clone(),
                model: consultation.model.clone(),
                input_tokens,
                output_tokens: usage.output_tokens,
                cost_micros: cost.unwrap_or(0),
            })
            .await?;
        state.usage.input_tokens += input_tokens;
        state.usage.output_tokens += usage.output_tokens;
        state.usage.add_cost(cost.unwrap_or(0), charged);
        ports
            .events
            .emit(RunEvent::Usage {
                input_tokens,
                output_tokens: usage.output_tokens,
                cost_micros: cost.map(|_| charged),
            })
            .await?;
    }
    Ok(())
}
