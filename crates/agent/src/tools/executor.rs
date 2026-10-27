//! Policy enforcement point (PEP) for tool calls. The model's request is
//! evaluated against the registry, the subject's permissions (with the agent
//! ceiling), the input schema and the autonomy policy before anything runs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mimorya_core::authz::{Actor, AuthorizationEngine, Subject};
use mimorya_core::autonomy::{AutonomyPolicy, ExecutionDecision, ToolRisk, decide_execution};
use mimorya_core::tools::{BoundAuthorization, ToolContext, ToolResult, ToolServices};
use serde_json::Value;
use uuid::Uuid;

use super::registry::{RegisteredTool, ToolRegistry};
use crate::llm::LlmToolCall;

const MAX_REPORTED_ISSUES: usize = 5;
/// Prefix of the refusal of a call whose input failed validation.
pub const INVALID_INPUT: &str = "Paramètres invalides";

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
    pub const ALLOW: &'static str = "allow";
    pub const REQUIRE_APPROVAL: &'static str = "require_approval";
    pub const DENY: &'static str = "deny";
    /// Every value of `kind`, as streamed and recorded in the tool log.
    pub const KINDS: [&'static str; 3] = [Self::ALLOW, Self::REQUIRE_APPROVAL, Self::DENY];

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Allow { .. } => Self::ALLOW,
            Self::RequireApproval { .. } => Self::REQUIRE_APPROVAL,
            Self::Deny { .. } => Self::DENY,
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
    services: ToolServices,
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
        services: ToolServices,
        timeout: Duration,
    ) -> Self {
        Self {
            engine,
            registry,
            subject,
            actor,
            autonomy,
            run_id,
            services,
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
                    reason: format!("{INVALID_INPUT} — {}", issues.join(" ; ")),
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
            deliverables: self.services.deliverables.clone(),
            accounts: self.services.accounts.clone(),
        };
        let started = Instant::now();
        let result = match tokio::time::timeout(self.timeout, tool.tool.execute(input, &ctx)).await
        {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => failure(&error.to_string()),
            Err(_) => failure(timed_out(tool)),
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

/// Why a call that ran out of time ended: one that does not only read may
/// have acted before it was stopped (ADR-119), and is not to be asked again
/// blindly.
fn timed_out(tool: &RegisteredTool) -> &'static str {
    match tool.info.risk {
        ToolRisk::Read => "Délai d’exécution dépassé",
        _ => {
            "Délai d’exécution dépassé : l’action a peut-être eu lieu ; vérifiez avant de la \
             redemander"
        }
    }
}

fn failure(message: &str) -> ToolResult {
    ToolResult::error(message).with_summary(format!("Échec : {message}"))
}
