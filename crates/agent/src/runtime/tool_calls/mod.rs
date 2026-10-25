//! Handling of the tool calls of one assistant turn: each call goes through
//! the policy enforcement point, with the rights of the current step, then is
//! denied, queued for human validation or executed; an allowed delegation
//! runs a sub-agent (`delegated.rs`, ADR-098). Every outcome is logged,
//! audited and streamed (`record.rs`).

mod delegated;
mod record;

use std::sync::Arc;
use std::time::Duration;

use mimorya_core::authz::Actor;
use mimorya_core::autonomy::AutonomyPolicy;
use serde_json::Value;
use uuid::Uuid;

use super::budget::BudgetLimits;
use super::delegation::{MAX_PER_TURN, SubAgents, TOO_MANY_AT_ONCE};
use super::ports::{CurrentAccess, EnginePorts, RunEvent};
use super::state::{PendingApproval, RunCheckpoint};
use super::tool_refusals::Requested;
use super::types::{RunConfig, RuntimeDeps};
use crate::llm::{LlmMessage, LlmToolCall, LlmToolResult};
use crate::tools::platform::DELEGATE_TASK;
use crate::tools::{RegisteredTool, ToolCallEvaluation, ToolExecutor, ToolRegistry};
use delegated::Delegated;
use record::Call;

#[derive(Debug, Default)]
pub struct TurnToolResults {
    /// Results of the calls that were denied or executed.
    pub results: Vec<LlmToolResult>,
    /// Calls waiting for a human decision (the run must pause when non-empty).
    pub awaiting: Vec<PendingApproval>,
}

pub struct ToolCallProcessor {
    registry: Arc<ToolRegistry>,
    actor: Actor,
    autonomy: AutonomyPolicy,
    run_id: Uuid,
    timeout: Duration,
    pub(super) ports: EnginePorts,
    /// What the run's sub-agents work with (ADR-098).
    sub_agents: SubAgents,
    /// The run's budget, of which each sub-agent gets a share.
    budget: BudgetLimits,
}

impl ToolCallProcessor {
    pub fn new(deps: &RuntimeDeps, config: &RunConfig) -> Self {
        Self {
            registry: deps.registry.clone(),
            actor: config.actor.clone(),
            autonomy: config.autonomy.clone(),
            run_id: config.run_id,
            timeout: deps.tool_timeout,
            ports: deps.ports.clone(),
            sub_agents: SubAgents::new(deps, config),
            budget: config.budget,
        }
    }

    pub fn tool(&self, name: &str) -> Option<RegisteredTool> {
        self.registry.get(name).cloned()
    }

    /// The policy enforcement point with the rights of the current step.
    fn executor(&self, access: &CurrentAccess) -> ToolExecutor {
        ToolExecutor::new(
            access.engine.clone(),
            self.registry.clone(),
            access.subject.clone(),
            self.actor.clone(),
            self.autonomy.clone(),
            self.run_id,
            self.ports.deliverables.clone(),
            self.timeout,
        )
    }

    pub async fn process_turn(
        &self,
        state: &mut RunCheckpoint,
        calls: &[LlmToolCall],
        access: &CurrentAccess,
    ) -> anyhow::Result<TurnToolResults> {
        let executor = self.executor(access);
        let mut turn = TurnToolResults::default();
        let mut delegations = Vec::new();
        for call in calls {
            let mut evaluation = executor.evaluate(call);
            if let ToolCallEvaluation::Allow { tool, .. } = &evaluation
                && is_delegation(tool)
                && delegations.len() >= MAX_PER_TURN
            {
                evaluation = ToolCallEvaluation::Deny {
                    tool: Some(tool.clone()),
                    reason: TOO_MANY_AT_ONCE.into(),
                };
            }
            let (title, reason, risk) = match &evaluation {
                ToolCallEvaluation::Allow { tool, .. } => {
                    (tool.info.title.clone(), None, Some(tool.info.risk))
                }
                ToolCallEvaluation::RequireApproval { tool, reason, .. } => (
                    tool.info.title.clone(),
                    Some(reason.clone()),
                    Some(tool.info.risk),
                ),
                ToolCallEvaluation::Deny { tool, reason } => (
                    tool.as_ref()
                        .map_or_else(|| call.name.clone(), |t| t.info.title.clone()),
                    Some(reason.clone()),
                    tool.as_ref().map(|t| t.info.risk),
                ),
            };
            self.ports
                .events
                .emit(RunEvent::ToolCall {
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    title,
                    decision: evaluation.kind().into(),
                    reason,
                    risk,
                })
                .await?;

            match evaluation {
                ToolCallEvaluation::Deny { reason, .. } => {
                    turn.results.push(self.deny(call, reason).await?)
                }
                ToolCallEvaluation::RequireApproval {
                    tool,
                    input,
                    reason,
                } => match self.request_approval(call, &tool, input, reason).await? {
                    Requested::Awaiting(pending) => turn.awaiting.push(pending),
                    Requested::Refused(result) => turn.results.push(result),
                },
                ToolCallEvaluation::Allow { tool, input } if is_delegation(&tool) => {
                    delegations.push(Delegated {
                        tool_call_id: call.id.clone(),
                        tool,
                        input,
                        approval_id: None,
                    });
                }
                ToolCallEvaluation::Allow { tool, input } => {
                    turn.results.push(
                        self.run(state, &tool, &call.id, input, None, access)
                            .await?,
                    );
                }
            }
        }
        if !delegations.is_empty() {
            // Side by side, once every other call of the turn is decided.
            turn.results
                .extend(self.delegate(state, delegations).await?);
            let position = |id: &str| calls.iter().position(|c| c.id == id);
            turn.results.sort_by_key(|r| position(&r.tool_call_id));
        }
        Ok(turn)
    }

    /// Executes an allowed (or human-approved) call, with the rights of the
    /// current step, and turns its result into model input.
    pub async fn run(
        &self,
        state: &mut RunCheckpoint,
        tool: &RegisteredTool,
        tool_call_id: &str,
        input: Value,
        approval_id: Option<Uuid>,
        access: &CurrentAccess,
    ) -> anyhow::Result<LlmToolResult> {
        if is_delegation(tool) {
            let delegated = Delegated {
                tool_call_id: tool_call_id.into(),
                tool: tool.clone(),
                input,
                approval_id,
            };
            let mut results = self.delegate(state, vec![delegated]).await?;
            return results
                .pop()
                .ok_or_else(|| anyhow::anyhow!("a delegation without its result"));
        }
        let (result, duration_ms) = self.executor(access).execute(tool, input.clone()).await;
        let call = Call {
            tool,
            tool_call_id,
            input,
            approval_id,
        };
        self.record(state, call, result, duration_ms, None).await
    }
}

fn is_delegation(tool: &RegisteredTool) -> bool {
    tool.info.name == DELEGATE_TASK
}

/// Tool results are returned in the order the model requested the calls.
pub fn order_by_request(
    state: &RunCheckpoint,
    mut results: Vec<LlmToolResult>,
) -> Vec<LlmToolResult> {
    let Some(calls) = state.messages.iter().rev().find_map(|m| match m {
        LlmMessage::Assistant { tool_calls, .. } => Some(tool_calls),
        _ => None,
    }) else {
        return results;
    };
    let position = |id: &str| calls.iter().position(|c| c.id == id).unwrap_or(0);
    results.sort_by_key(|r| position(&r.tool_call_id));
    results
}
