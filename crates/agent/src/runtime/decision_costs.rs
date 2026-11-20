//! What a run's decisions cost (ADR-213), added to its totals as its own
//! calls are, and told to its listeners: the router's choice (D1) and the
//! skills listed (D2). A memory read before it is saved (D3) is counted in
//! the company's consumption only, its port reaching no run's totals. An
//! action read by the guard (D5) and an external content screened (D6) are
//! counted in the run that asked.

use super::ports::{EnginePorts, RunEvent};
use super::state::RunCheckpoint;
use crate::decision::Counted;

/// The calls that answered, added to the run's totals.
pub(super) async fn count(
    ports: &EnginePorts,
    state: &mut RunCheckpoint,
    counted: &[Counted],
) -> anyhow::Result<()> {
    for call in counted {
        state.usage.input_tokens += call.input_tokens;
        state.usage.output_tokens += call.output_tokens;
        state
            .usage
            .add_cost(call.cost_micros.unwrap_or(0), call.charged_micros);
        ports
            .events
            .emit(RunEvent::Usage {
                input_tokens: call.input_tokens,
                output_tokens: call.output_tokens,
                cost_micros: call.cost_micros.map(|_| call.charged_micros),
            })
            .await?;
    }
    Ok(())
}
