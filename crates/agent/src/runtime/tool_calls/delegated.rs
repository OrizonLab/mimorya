//! The allowed delegations of a turn (ADR-098): each runs a sub-agent in a
//! child run, side by side with the others, on an equal share of what is
//! left of the run's budget; what each spent then counts in the run, and
//! its outcome is recorded like any call's, naming the child run.

use futures::future::join_all;
use mimorya_core::tools::ToolResult;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Call, ToolCallProcessor};
use crate::llm::LlmToolResult;
use crate::runtime::budget::share_left;
use crate::runtime::delegation::{SUB_AGENT_BUDGET, SubAgentReport};
use crate::runtime::state::{RunCheckpoint, RunUsageTotals};
use crate::tools::RegisteredTool;
use crate::tools::executor::INVALID_INPUT;
use crate::tools::platform::DelegateTaskInput;

/// An allowed (or approved) call of `delegate_task`, its input validated.
pub(super) struct Delegated {
    pub tool_call_id: String,
    pub tool: RegisteredTool,
    pub input: Value,
    pub approval_id: Option<Uuid>,
}

impl ToolCallProcessor {
    /// Runs `delegations` side by side; their results, in the same order.
    pub(super) async fn delegate(
        &self,
        state: &mut RunCheckpoint,
        delegations: Vec<Delegated>,
    ) -> anyhow::Result<Vec<LlmToolResult>> {
        let parts = u32::try_from(delegations.len()).unwrap_or(u32::MAX);
        let share = share_left(
            &self.budget,
            &state.usage,
            state.elapsed_ms,
            parts,
            &SUB_AGENT_BUDGET,
        );
        let system = state.system.clone();
        let runs = delegations.iter().map(|delegated| {
            let system = &system;
            async move {
                match serde_json::from_value::<DelegateTaskInput>(delegated.input.clone()) {
                    Ok(input) => self
                        .sub_agents
                        .run(system, &input, share)
                        .await
                        .map(|report| (Some(input), report)),
                    Err(error) => Ok((None, unreadable(&error))),
                }
            }
        });
        let reports = join_all(runs).await;
        let mut results = Vec::with_capacity(delegations.len());
        for (delegated, report) in delegations.into_iter().zip(reports) {
            let (input, report) = report?;
            add_spent(&mut state.usage, &report.usage);
            let mut metadata = json!({});
            if let Some(input) = input {
                metadata["profile"] = json!(input.profile.as_str());
            }
            if let Some(run_id) = report.run_id {
                metadata["subRunId"] = json!(run_id);
            }
            let call = Call {
                tool: &delegated.tool,
                tool_call_id: &delegated.tool_call_id,
                input: delegated.input,
                approval_id: delegated.approval_id,
            };
            let result = self
                .record(
                    state,
                    call,
                    report.result,
                    report.duration_ms,
                    Some(metadata),
                )
                .await?;
            results.push(result);
        }
        Ok(results)
    }
}

/// What a sub-agent spent, counted in its parent's budget: its tokens and
/// its cost, not its steps.
fn add_spent(usage: &mut RunUsageTotals, spent: &RunUsageTotals) {
    usage.input_tokens += spent.input_tokens;
    usage.output_tokens += spent.output_tokens;
    usage.cost_micros += spent.cost_micros;
}

/// A delegation whose validated input does not read back: never run.
fn unreadable(error: &serde_json::Error) -> SubAgentReport {
    SubAgentReport {
        run_id: None,
        result: ToolResult::error(format!("{INVALID_INPUT} — {error}")).with_summary("Refusé"),
        usage: RunUsageTotals::default(),
        duration_ms: 0,
    }
}
