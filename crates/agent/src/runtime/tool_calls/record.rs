//! What becomes of a call that ran (or was meant to): logged, audited and
//! streamed, counted in the run, its citations among the run's evidence, the
//! quality criteria of a skill `skill_read` loaded among the run's (ADR-106),
//! and its result turned into model input.

use mimorya_core::tools::ToolResult;
use serde_json::{Value, json};
use uuid::Uuid;

use super::ToolCallProcessor;
use crate::llm::LlmToolResult;
use crate::runtime::ports::{EngineAuditEntry, RunEvent, ToolInvocationRecord};
use crate::runtime::state::RunCheckpoint;
use crate::tools::platform::SKILL_READ;
use crate::tools::{RegisteredTool, ToolCallEvaluation, wrap_tool_result};

/// Quality criteria an answer is reviewed on, at most, whatever the skills
/// loaded: two skills' worth.
const MAX_SKILL_CRITERIA: usize = 16;

/// A call whose outcome is recorded.
pub(super) struct Call<'a> {
    pub tool: &'a RegisteredTool,
    pub tool_call_id: &'a str,
    pub input: Value,
    pub approval_id: Option<Uuid>,
}

impl ToolCallProcessor {
    /// Logs, audits and streams the outcome of `call`, counted in the run,
    /// and turns its result into model input; `metadata` adds to the audit.
    pub(super) async fn record(
        &self,
        state: &mut RunCheckpoint,
        call: Call<'_>,
        result: ToolResult,
        duration_ms: u64,
        metadata: Option<Value>,
    ) -> anyhow::Result<LlmToolResult> {
        let Call {
            tool,
            tool_call_id,
            input,
            approval_id,
        } = call;
        state.usage.tool_calls += 1;
        if !result.is_error {
            state.evidence.extend(result.citations.iter().cloned());
            // Tool names are unique: no other tool can raise the bar.
            if tool.info.name == SKILL_READ {
                take_skill_criteria(state, &result.quality_criteria);
            }
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
                    ToolCallEvaluation::REQUIRE_APPROVAL
                } else {
                    ToolCallEvaluation::ALLOW
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
        let mut audited = json!({ "risk": tool.info.risk, "durationMs": duration_ms });
        if let Some(id) = approval_id {
            audited["approvalId"] = json!(id);
        }
        if let Some(Value::Object(extra)) = metadata {
            for (key, value) in extra {
                audited[key] = value;
            }
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
                metadata: audited,
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
            content: wrap_tool_result(name, &result.content, result.is_error),
            is_error: result.is_error,
        })
    }
}

fn error_text(content: &Value, summary: &str) -> String {
    content["erreur"]
        .as_str()
        .map_or_else(|| summary.to_string(), str::to_string)
}

/// The criteria of a skill loaded, added to the run's, each once.
fn take_skill_criteria(state: &mut RunCheckpoint, criteria: &[String]) {
    for criterion in criteria {
        if state.skill_criteria.len() >= MAX_SKILL_CRITERIA {
            break;
        }
        if !state.skill_criteria.contains(criterion) {
            state.skill_criteria.push(criterion.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SKILL_CRITERIA, take_skill_criteria};
    use crate::runtime::state::RunCheckpoint;

    #[test]
    fn a_skills_criteria_are_taken_each_once_and_sixteen_at_most() {
        let mut state = RunCheckpoint::new("SYSTEM".into(), Vec::new());
        let first: Vec<String> = (0..10).map(|i| format!("Critère {i}.")).collect();
        take_skill_criteria(&mut state, &first);
        take_skill_criteria(&mut state, &first);
        assert_eq!(state.skill_criteria, first);
        let second: Vec<String> = (5..15).map(|i| format!("Critère {i}.")).collect();
        take_skill_criteria(&mut state, &second);
        assert_eq!(state.skill_criteria.len(), 15);
        let third: Vec<String> = (20..25).map(|i| format!("Critère {i}.")).collect();
        take_skill_criteria(&mut state, &third);
        assert_eq!(state.skill_criteria.len(), MAX_SKILL_CRITERIA);
        assert_eq!(state.skill_criteria.last().unwrap(), "Critère 20.");
    }
}
