//! A run's Anthropic advisor (ADR-207, step 3): the escalation model, which
//! the main model consults while it answers. It is decided at the run's
//! first call: given while the company has it and the run starts on its
//! main model within its bounds; never added later, nor changed, since that
//! would edit the tools the run's reasoning is bound to. Anthropic bounds
//! the consultations of a turn by the escalations a run may have; so that
//! the run's bound holds, the tool is withdrawn once for good as soon as
//! the run has an escalation that count does not know: a consultation of an
//! earlier turn (the platform's message opens a new one), or a call to the
//! escalation model. It is withdrawn too when a bound is reached (journaled
//! once, as an escalation's) or the company's escalation changed. Then the
//! reasoning bound to it is dropped rather than refused; never while a
//! consultation is still to run (the turn paused, or the advisor called
//! beside tools): the next request gives it, on the main model, a skill's
//! switch to the escalation model waiting for the call after. Each
//! consultation is an escalation: counted in the run's, recorded and priced
//! as its model.

use super::escalation::{bound_reached, limited};
use super::ports::{EnginePorts, RunEvent, UsageRecord};
use super::state::{AdvisorState, RunCheckpoint};
use super::types::RuntimeDeps;
use crate::llm::{
    AdvisorTool, LlmMessage, LlmResponse, ModelRole, PriceTable, StopReason, advisor_tool,
};

/// The advisor of a call, and whether the reasoning bound to it before it
/// was withdrawn is dropped.
#[derive(Debug, Default)]
pub(super) struct AdvisorCall {
    pub tool: Option<AdvisorTool>,
    pub drop_unbound_thinking: bool,
}

/// The advisor of the run's next call, its model already chosen (an
/// escalated call counted in the run's escalations).
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
    // A consultation still to run is given the tool it was asked with.
    if state.escalation.advisor_pending {
        return Ok(given(tool));
    }
    // The platform's message opens a turn, whose consultations Anthropic
    // counts from none.
    if matches!(state.messages.last(), Some(LlmMessage::User { .. })) {
        state.escalation.turn_consultations = 0;
    }
    if !still_given(deps, state, &tool).await? {
        state.escalation.advisor = AdvisorState::Withdrawn;
        return Ok(AdvisorCall {
            tool: None,
            drop_unbound_thinking: true,
        });
    }
    Ok(given(tool))
}

fn given(tool: AdvisorTool) -> AdvisorCall {
    AdvisorCall {
        tool: Some(tool),
        drop_unbound_thinking: false,
    }
}

/// At the run's first call: given while the company has it and the run
/// starts on its main model, within its bounds; a run checkpointed before
/// the advisor existed, which made calls already, goes on without.
async fn decided(deps: &RuntimeDeps, state: &RunCheckpoint) -> anyhow::Result<AdvisorState> {
    let Some(escalation) = deps.escalation.as_ref() else {
        return Ok(AdvisorState::Off);
    };
    let Some(model) = escalation.advisor.as_deref() else {
        return Ok(AdvisorState::Off);
    };
    if state.usage.steps > 0 || state.escalation.escalated {
        return Ok(AdvisorState::Off);
    }
    // Nothing escalated: a bound reached is not journaled here.
    if bound_reached(escalation, state).await?.is_some() {
        return Ok(AdvisorState::Off);
    }
    Ok(AdvisorState::Given(advisor_tool(
        model,
        escalation.max_per_run,
    )))
}

/// Whether the advisor given stays: the company still has it, the same; the
/// run's escalations are all consultations of this turn, which Anthropic
/// bounds; and its bounds allow another.
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
    if state.escalation.calls > state.escalation.turn_consultations {
        return Ok(false);
    }
    match bound_reached(escalation, state).await? {
        Some(bound) => {
            limited(deps, state, bound).await?;
            Ok(false)
        }
        None => Ok(true),
    }
}

/// After a call: its consultations counted in the run's escalations and in
/// its turn's, and whether one is still to run: the turn paused, or the
/// model called its advisor beside tools, the consultation running at the
/// start of the next request.
pub(super) fn after_call(state: &mut RunCheckpoint, response: &LlmResponse) {
    let consulted = u32::try_from(response.consultations.len()).unwrap_or(u32::MAX);
    state.escalation.calls = state.escalation.calls.saturating_add(consulted);
    state.escalation.turn_consultations = state
        .escalation
        .turn_consultations
        .saturating_add(consulted);
    let pending = response.advisor_pending || response.stop_reason == StopReason::PauseTurn;
    state.escalation.advisor_pending =
        pending && matches!(state.escalation.advisor, AdvisorState::Given(_));
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
