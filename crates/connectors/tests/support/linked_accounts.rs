//! The tools of a linked account as a run calls them (ADR-117, ADR-122):
//! behind the policy enforcement point, with the accounts the run's person
//! linked, or none reachable; the test network, where the local providers
//! are reachable.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolExecutor, ToolRegistry};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_core::authz::Actor;
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use mimorya_core::demo;
use mimorya_core::tools::{AccountAccessError, LinkedAccounts, ToolRef, ToolResult, ToolServices};
use serde_json::Value;
use uuid::Uuid;

/// The test network: the local providers are reachable.
pub const LOCAL: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
};

/// The person's accounts: none reachable, for this reason.
pub struct Unreachable(pub AccountAccessError);

#[async_trait]
impl LinkedAccounts for Unreachable {
    async fn access_token(
        &self,
        _connector_type: &str,
        _needs: &[&str],
    ) -> Result<String, AccountAccessError> {
        Err(self.0.clone())
    }
}

/// The tools behind the policy enforcement point, for `user_id`, reaching
/// these accounts.
pub fn mailbox_executor(
    tools: Vec<ToolRef>,
    user_id: Uuid,
    ports: &Arc<InMemoryPorts>,
    accounts: Arc<dyn LinkedAccounts>,
) -> ToolExecutor {
    let run_id = Uuid::new_v4();
    ToolExecutor::new(
        Arc::new(demo::authorization_engine()),
        Arc::new(ToolRegistry::new(tools, &HashMap::new()).unwrap()),
        Arc::new(demo::subject(user_id)),
        Actor::agent(Some(run_id)),
        AutonomyPolicy::new(AutonomyLevel::ExecuteWithApproval),
        run_id,
        ToolServices {
            deliverables: ports.ports().deliverables,
            accounts,
        },
        Duration::from_secs(30),
    )
}

/// The call evaluated, then run: the tools read without validation.
pub async fn call(executor: &ToolExecutor, name: &str, input: Value) -> ToolResult {
    let evaluation = executor.evaluate(&LlmToolCall {
        id: "c".into(),
        name: name.into(),
        input,
    });
    match evaluation {
        ToolCallEvaluation::Allow { tool, input } => executor.execute(&tool, input).await.0,
        ToolCallEvaluation::RequireApproval { reason, .. }
        | ToolCallEvaluation::Deny { reason, .. } => panic!("{name} not allowed: {reason}"),
    }
}

/// The call evaluated, which needs a validation, then run as validated.
pub async fn validated_call(executor: &ToolExecutor, name: &str, input: Value) -> ToolResult {
    let evaluation = executor.evaluate(&LlmToolCall {
        id: "c".into(),
        name: name.into(),
        input,
    });
    match evaluation {
        ToolCallEvaluation::RequireApproval { tool, input, .. } => {
            executor.execute(&tool, input).await.0
        }
        ToolCallEvaluation::Allow { .. } => panic!("{name} ran without validation"),
        ToolCallEvaluation::Deny { reason, .. } => panic!("{name} denied: {reason}"),
    }
}

/// The error a failed result tells the model.
pub fn error_of(result: &ToolResult) -> String {
    assert!(result.is_error, "{:?}", result.content);
    result.content["erreur"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}
