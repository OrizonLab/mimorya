//! Tool calls that do not run: refused by the policy enforcement point,
//! queued for human validation (or refused when nobody may validate them,
//! ADR-059), refused by the user, whose request expired or whose tool was
//! removed before anyone decided, or abandoned when the run may no longer act
//! for its user (ADR-035). Each is logged, audited or streamed as it ends,
//! and reported to the model when the run goes on.

use serde_json::{Value, json};

use super::ports::{
    ApprovalPlacement, ApprovalRequest, EngineAuditEntry, RunEvent, ToolInvocationRecord,
};
use super::state::PendingApproval;
use super::tool_calls::ToolCallProcessor;
use super::types::ApprovalDecision;
use crate::llm::{LlmToolCall, LlmToolResult};
use crate::tools::executor::INVALID_INPUT;
use crate::tools::{RegisteredTool, ToolCallEvaluation};

const TOOL_REMOVED: &str = "Outil désactivé depuis la demande de validation";
const REQUEST_EXPIRED: &str = "Demande de validation expirée";
const REFUSED: &str = "Action refusée lors de la validation";
const NOBODY_VALIDATES: &str = "Aucun responsable habilité à valider cette action";

/// A call queued for validation, or refused at once when nobody may
/// validate it (ADR-059).
pub(super) enum Requested {
    Awaiting(PendingApproval),
    Refused(LlmToolResult),
}

/// Why a decided call does not run (its tool removed since, its request
/// expired before anyone decided, or refused): the reason the model gets,
/// and what the live view shows, without what the user wrote.
pub(super) fn why_not_run(
    tool_present: bool,
    decision: &ApprovalDecision,
) -> (String, &'static str) {
    if !tool_present {
        (TOOL_REMOVED.into(), TOOL_REMOVED)
    } else if decision.expired {
        let reason = format!("{REQUEST_EXPIRED} : l’action n’a pas été exécutée");
        (reason, REQUEST_EXPIRED)
    } else {
        let comment = decision
            .comment
            .as_ref()
            .map(|c| format!(" : {c}"))
            .unwrap_or_default();
        (format!("{REFUSED}{comment}"), REFUSED)
    }
}

impl ToolCallProcessor {
    /// A decided call that does not run (refused by a human, its request
    /// expired, its tool removed, or abandoned) is reported to the model, and
    /// ended in the live view with `shown`, the reason without what the user
    /// wrote: it ran nothing, and is never taken for a call a stop of the
    /// service left half done.
    pub async fn reject(
        &self,
        approval: &PendingApproval,
        reason: String,
        shown: &str,
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
        self.ports
            .events
            .emit(RunEvent::ToolResult {
                tool_call_id: approval.tool_call_id.clone(),
                tool_name: approval.tool_name.clone(),
                summary: shown.into(),
                is_error: true,
                duration_ms: 0,
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
    ) -> anyhow::Result<Requested> {
        let action_summary = tool
            .tool
            .describe_action(&input)
            .unwrap_or_else(|| format!("{} ({})", tool.info.title, call.name));
        let placement = self
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
        let approval_id = match placement {
            ApprovalPlacement::Pending(id) => id,
            ApprovalPlacement::Unroutable(why) => {
                return Ok(Requested::Refused(
                    self.unroutable(call, tool, input, why).await?,
                ));
            }
        };
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
        Ok(Requested::Awaiting(PendingApproval {
            approval_id,
            tool_call_id: call.id.clone(),
            tool_name: tool.info.name.clone(),
            input,
        }))
    }

    /// A call nobody may validate: it does not run, and ends in the live view
    /// as refused, its reason given to the model.
    async fn unroutable(
        &self,
        call: &LlmToolCall,
        tool: &RegisteredTool,
        input: Value,
        reason: String,
    ) -> anyhow::Result<LlmToolResult> {
        self.ports
            .tool_log
            .record(ToolInvocationRecord {
                tool_call_id: call.id.clone(),
                tool_name: tool.info.name.clone(),
                input,
                decision: ToolCallEvaluation::REQUIRE_APPROVAL,
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
                action: "agent.approval.unroutable".into(),
                outcome: "denied",
                resource_type: Some("tool".into()),
                resource_id: Some(tool.info.name.clone()),
                reason: Some(reason.clone()),
                metadata: json!({ "risk": tool.info.risk }),
            })
            .await?;
        self.ports
            .events
            .emit(RunEvent::ToolResult {
                tool_call_id: call.id.clone(),
                tool_name: tool.info.name.clone(),
                summary: NOBODY_VALIDATES.into(),
                is_error: true,
                duration_ms: 0,
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
        self.reject(approval, reason.to_string(), reason).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_why_a_decided_call_does_not_run_never_showing_what_the_user_wrote() {
        let refused = ApprovalDecision::rejected(Some("pas cette semaine".into()));
        assert_eq!(
            why_not_run(true, &refused),
            (format!("{REFUSED} : pas cette semaine"), REFUSED)
        );
        assert_eq!(
            why_not_run(true, &ApprovalDecision::rejected(None)),
            (REFUSED.into(), REFUSED)
        );
        let (reason, shown) = why_not_run(true, &ApprovalDecision::expired());
        assert_eq!(shown, REQUEST_EXPIRED);
        assert!(reason.starts_with(REQUEST_EXPIRED) && !reason.contains("refusée"));
        // A tool removed since comes first, whatever was decided.
        for decision in [
            refused,
            ApprovalDecision::expired(),
            ApprovalDecision::approved(),
        ] {
            assert_eq!(
                why_not_run(false, &decision),
                (TOOL_REMOVED.into(), TOOL_REMOVED)
            );
        }
    }
}
