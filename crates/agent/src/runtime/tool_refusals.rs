//! Tool calls that do not run: refused by the policy enforcement point,
//! queued for human validation, refused by the user, or abandoned when the
//! run may no longer act for its user (ADR-035). Each is logged, audited or
//! streamed as it ends, and reported to the model when the run goes on.

use serde_json::{Value, json};

use super::ports::{ApprovalRequest, EngineAuditEntry, RunEvent, ToolInvocationRecord};
use super::state::PendingApproval;
use super::tool_calls::ToolCallProcessor;
use super::types::ApprovalDecision;
use crate::llm::{LlmToolCall, LlmToolResult};
use crate::tools::executor::INVALID_INPUT;
use crate::tools::{RegisteredTool, ToolCallEvaluation};

impl ToolCallProcessor {
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
                decision: ToolCallEvaluation::REQUIRE_APPROVAL,
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

    pub(super) async fn deny(
        &self,
        call: &LlmToolCall,
        reason: String,
    ) -> anyhow::Result<LlmToolResult> {
        self.ports
            .tool_log
            .record(ToolInvocationRecord {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                input: call.input.clone(),
                decision: ToolCallEvaluation::DENY,
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

    pub(super) async fn request_approval(
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
                decision: ToolCallEvaluation::REQUIRE_APPROVAL,
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

    /// A validated call left unrun because the run stops: the decision is
    /// announced, and the call logged with why it did not run.
    pub async fn abandon(
        &self,
        approval: &PendingApproval,
        decision: &ApprovalDecision,
        reason: &str,
    ) -> anyhow::Result<()> {
        self.ports
            .events
            .emit(RunEvent::ApprovalResolved {
                approval_id: approval.approval_id,
                approved: decision.approved,
            })
            .await?;
        self.reject(approval, reason.to_string()).await?;
        Ok(())
    }
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
