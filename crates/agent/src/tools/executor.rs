//! Policy enforcement point (PEP) for tool calls. The model's request is
//! evaluated against the registry, the subject's permissions (with the agent
//! ceiling), the input schema and the autonomy policy before anything runs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mimorya_core::authz::{Actor, AuthorizationEngine, Subject};
use mimorya_core::autonomy::{AutonomyPolicy, ExecutionDecision, decide_execution};
use mimorya_core::tools::{BoundAuthorization, DeliverableService, ToolContext, ToolResult};
use serde_json::Value;
use uuid::Uuid;

use super::registry::{RegisteredTool, ToolRegistry};
use crate::llm::LlmToolCall;

const MAX_REPORTED_ISSUES: usize = 5;

pub enum ToolCallEvaluation {
    Allow {
        tool: RegisteredTool,
        input: Value,
    },
    RequireApproval {
        tool: RegisteredTool,
        input: Value,
        reason: String,
    },
    Deny {
        tool: Option<RegisteredTool>,
        reason: String,
    },
}

impl ToolCallEvaluation {
    /// `allow`, `require_approval` or `deny`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Allow { .. } => "allow",
            Self::RequireApproval { .. } => "require_approval",
            Self::Deny { .. } => "deny",
        }
    }
}

#[derive(Clone)]
pub struct ToolExecutor {
    engine: Arc<AuthorizationEngine>,
    registry: Arc<ToolRegistry>,
    subject: Arc<Subject>,
    actor: Actor,
    autonomy: AutonomyPolicy,
    run_id: Uuid,
    deliverables: Arc<dyn DeliverableService>,
    timeout: Duration,
}

impl ToolExecutor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine: Arc<AuthorizationEngine>,
        registry: Arc<ToolRegistry>,
        subject: Arc<Subject>,
        actor: Actor,
        autonomy: AutonomyPolicy,
        run_id: Uuid,
        deliverables: Arc<dyn DeliverableService>,
        timeout: Duration,
    ) -> Self {
        Self {
            engine,
            registry,
            subject,
            actor,
            autonomy,
            run_id,
            deliverables,
            timeout,
        }
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    pub fn evaluate(&self, call: &LlmToolCall) -> ToolCallEvaluation {
        let Some(tool) = self.registry.get(&call.name).cloned() else {
            return ToolCallEvaluation::Deny {
                tool: None,
                reason: format!("Outil « {} » inconnu ou désactivé", call.name),
            };
        };
        if let Some(missing) = self.missing_permission(&tool) {
            return ToolCallEvaluation::Deny {
                tool: Some(tool),
                reason: format!("Permission {missing} requise"),
            };
        }
        let input = match tool.tool.parse_input(&call.input) {
            Ok(input) => input,
            Err(issues) => {
                let issues: Vec<String> = issues.into_iter().take(MAX_REPORTED_ISSUES).collect();
                return ToolCallEvaluation::Deny {
                    tool: Some(tool),
                    reason: format!("Paramètres invalides — {}", issues.join(" ; ")),
                };
            }
        };
        match decide_execution(&self.autonomy, tool.risk_profile()) {
            ExecutionDecision::Deny { reason } => ToolCallEvaluation::Deny {
                tool: Some(tool),
                reason,
            },
            ExecutionDecision::RequireApproval { reason } => ToolCallEvaluation::RequireApproval {
                tool,
                input,
                reason,
            },
            ExecutionDecision::Allow => ToolCallEvaluation::Allow { tool, input },
        }
    }

    /// Executes a tool whose call has been allowed (or approved by a human).
    /// Permissions are checked again: they may have changed while the call was
    /// waiting for validation. Returns the result and its duration.
    pub async fn execute(&self, tool: &RegisteredTool, input: Value) -> (ToolResult, u64) {
        if let Some(missing) = self.missing_permission(tool) {
            return (
                ToolResult::error(format!("Permission {missing} requise")).with_summary("Refusé"),
                0,
            );
        }
        let ctx = ToolContext {
            tenant_id: self.subject.tenant_id,
            subject: self.subject.clone(),
            actor: self.actor.clone(),
            run_id: self.run_id,
            authz: BoundAuthorization::new(
                self.engine.clone(),
                self.subject.clone(),
                self.actor.clone(),
            ),
            deliverables: self.deliverables.clone(),
        };
        let started = Instant::now();
        let result = match tokio::time::timeout(self.timeout, tool.tool.execute(input, &ctx)).await
        {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => failure(&error.to_string()),
            Err(_) => failure("Délai d’exécution dépassé"),
        };
        (result, started.elapsed().as_millis() as u64)
    }

    fn missing_permission(&self, tool: &RegisteredTool) -> Option<String> {
        tool.info
            .required_permissions
            .iter()
            .find(|p| !self.engine.has(&self.subject, p, &self.actor))
            .cloned()
    }
}

fn failure(message: &str) -> ToolResult {
    ToolResult::error(message).with_summary(format!("Échec : {message}"))
}
