//! Handling of the tool calls of one assistant turn: each call goes through
//! the policy enforcement point, with the rights of the current step, then is
//! denied, queued for human validation or executed. Every outcome is logged,
//! audited and streamed.

use std::sync::Arc;
use std::time::Duration;

use mimorya_core::authz::Actor;
use mimorya_core::autonomy::AutonomyPolicy;
use serde_json::{Value, json};
use uuid::Uuid;

use super::ports::{
    ApprovalRequest, CurrentAccess, EngineAuditEntry, EnginePorts, RunEvent, ToolInvocationRecord,
};
use super::state::{PendingApproval, RunCheckpoint};
use super::types::{RunConfig, RuntimeDeps};
use crate::llm::{LlmMessage, LlmToolCall, LlmToolResult};
use crate::tools::executor::INVALID_INPUT;
use crate::tools::{
    RegisteredTool, ToolCallEvaluation, ToolExecutor, ToolRegistry, wrap_untrusted,
};

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
    ports: EnginePorts,
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
        for call in calls {
            let evaluation = executor.evaluate(call);
            let (title, reason) = match &evaluation {
                ToolCallEvaluation::Allow { tool, .. } => (tool.info.title.clone(), None),
                ToolCallEvaluation::RequireApproval { tool, reason, .. } => {
                    (tool.info.title.clone(), Some(reason.clone()))
                }
                ToolCallEvaluation::Deny { tool, reason } => (
                    tool.as_ref()
                        .map_or_else(|| call.name.clone(), |t| t.info.title.clone()),
                    Some(reason.clone()),
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
                } => {
                    turn.awaiting
                        .push(self.request_approval(call, &tool, input, reason).await?);
                }
                ToolCallEvaluation::Allow { tool, input } => {
                    turn.results.push(
                        self.run(state, &tool, &call.id, input, None, access)
                            .await?,
                    );
                }
            }
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
        let (result, duration_ms) = self.executor(access).execute(tool, input.clone()).await;
        state.usage.tool_calls += 1;
        if !result.is_error {
            state.evidence.extend(result.citations.iter().cloned());
        }
        let summary = result.summary.clone().unwrap_or_else(|| {
            if result.is_error {
                "Échec".into()
            } else {
                "Terminé".into()
            }
        });
        let name = &tool.info.name;

        self.ports
            .tool_log
            .record(ToolInvocationRecord {
                tool_call_id: tool_call_id.into(),
                tool_name: name.clone(),
                input,
                decision: if approval_id.is_some() {
                    "require_approval"
                } else {
                    "allow"
                },
                decision_reason: None,
                status: if result.is_error {
                    "failed"
                } else {
                    "succeeded"
                },
                output_summary: Some(summary.clone()),
                error: result
                    .is_error
                    .then(|| error_text(&result.content, &summary)),
                duration_ms: Some(duration_ms),
                approval_id,
            })
            .await?;
        let mut metadata = json!({ "risk": tool.info.risk, "durationMs": duration_ms });
        if let Some(id) = approval_id {
            metadata["approvalId"] = json!(id);
        }
        self.ports
            .audit
            .record(EngineAuditEntry {
                action: "agent.tool.execute".into(),
                outcome: if result.is_error {
                    "failure"
                } else {
                    "success"
                },
                resource_type: Some("tool".into()),
                resource_id: Some(name.clone()),
                reason: None,
                metadata,
            })
            .await?;
        self.ports
            .events
            .emit(RunEvent::ToolResult {
                tool_call_id: tool_call_id.into(),
                tool_name: name.clone(),
                summary,
                is_error: result.is_error,
                duration_ms,
            })
            .await?;
        Ok(LlmToolResult {
            tool_call_id: tool_call_id.into(),
            content: wrap_untrusted(name, &result.content),
            is_error: result.is_error,
        })
    }

    /// A call refused by a human (or whose tool disappeared) is reported to the model.
    pub async fn reject(
        &self,
        approval: &PendingApproval,
        reason: String,
    ) -> anyhow::Result<LlmToolResult> {
        self.ports
            .tool_log
            .record(ToolInvocationRecord {
                tool_call_id: approval.tool_call_id.clone(),
                tool_name: approval.tool_name.clone(),
                input: approval.input.clone(),
                decision: "require_approval",
                decision_reason: Some(reason.clone()),
                status: "rejected",
                output_summary: None,
                error: None,
                duration_ms: None,
                approval_id: Some(approval.approval_id),
            })
            .await?;
        Ok(LlmToolResult {
            tool_call_id: approval.tool_call_id.clone(),
            content: json!({ "erreur": reason }).to_string(),
            is_error: true,
        })
    }

    async fn deny(&self, call: &LlmToolCall, reason: String) -> anyhow::Result<LlmToolResult> {
        self.ports
            .tool_log
            .record(ToolInvocationRecord {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                input: call.input.clone(),
                decision: "deny",
                decision_reason: Some(reason.clone()),
                status: "denied",
                output_summary: None,
                error: None,
                duration_ms: None,
                approval_id: None,
            })
            .await?;
        self.ports
            .audit
            .record(EngineAuditEntry {
                action: "agent.tool.denied".into(),
                outcome: "denied",
                resource_type: Some("tool".into()),
                resource_id: Some(call.name.clone()),
                reason: Some(audit_reason(&reason)),
                metadata: json!({}),
            })
            .await?;
        Ok(LlmToolResult {
            tool_call_id: call.id.clone(),
            content: json!({
                "erreur": format!("Refusé par la plateforme : {reason}"),
                "consigne": "Ne tente pas de contourner ce refus ; adapte ta réponse ou explique la limite.",
            })
            .to_string(),
            is_error: true,
        })
    }

    async fn request_approval(
        &self,
        call: &LlmToolCall,
        tool: &RegisteredTool,
        input: Value,
        reason: String,
    ) -> anyhow::Result<PendingApproval> {
        let action_summary = tool
            .tool
            .describe_action(&input)
            .unwrap_or_else(|| format!("{} ({})", tool.info.title, call.name));
        let approval_id = self
            .ports
            .approvals
            .create(ApprovalRequest {
                tool_name: tool.info.name.clone(),
                tool_call_id: call.id.clone(),
                action_summary: action_summary.clone(),
                payload: input.clone(),
                risk: tool.info.risk,
                reason: reason.clone(),
            })
            .await?;
        self.ports
            .tool_log
            .record(ToolInvocationRecord {
                tool_call_id: call.id.clone(),
                tool_name: tool.info.name.clone(),
                input: input.clone(),
                decision: "require_approval",
                decision_reason: Some(reason.clone()),
                status: "awaiting_approval",
                output_summary: None,
                error: None,
                duration_ms: None,
                approval_id: Some(approval_id),
            })
            .await?;
        self.ports
            .audit
            .record(EngineAuditEntry {
                action: "agent.approval.requested".into(),
                outcome: "success",
                resource_type: Some("approval".into()),
                resource_id: Some(approval_id.to_string()),
                reason: Some(reason),
                metadata: json!({ "toolName": tool.info.name, "risk": tool.info.risk }),
            })
            .await?;
        self.ports
            .events
            .emit(RunEvent::ApprovalRequested {
                approval_id,
                tool_name: tool.info.name.clone(),
                action_summary,
                risk: tool.info.risk,
            })
            .await?;
        Ok(PendingApproval {
            approval_id,
            tool_call_id: call.id.clone(),
            tool_name: tool.info.name.clone(),
            input,
        })
    }
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

fn error_text(content: &Value, summary: &str) -> String {
    content["erreur"]
        .as_str()
        .map_or_else(|| summary.to_string(), str::to_string)
}

/// The audit keeps why a call was refused, never the input it carried
/// (validation messages may quote it); the tool log keeps the detail.
fn audit_reason(reason: &str) -> String {
    if reason.starts_with(INVALID_INPUT) {
        INVALID_INPUT.to_string()
    } else {
        reason.to_string()
    }
}
