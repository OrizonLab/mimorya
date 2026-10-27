//! The Microsoft 365 tools as a run calls them (ADR-117): the connector
//! started on the test Graph, its tools reading or writing too (ADR-119),
//! behind the policy enforcement point, with the accounts the run's person
//! linked.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolExecutor, ToolRegistry};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::{InstanceContext, connector_type};
use mimorya_core::account_links::MICROSOFT365;
use mimorya_core::authz::Actor;
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use mimorya_core::demo;
use mimorya_core::tools::{AccountAccessError, LinkedAccounts, ToolRef, ToolResult, ToolServices};
use serde_json::{Value, json};
use uuid::Uuid;

/// The test network: the local Graph is reachable.
pub const LOCAL: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
};

/// The company's application, its Graph the test one.
pub fn application(graph: &str) -> Value {
    json!({
        "directory": "72f988bf-86f1-41af-91ab-2d7cd011db47",
        "clientId": "0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0",
        "issuer": "http://127.0.0.1:9/v2.0",
        "graph": graph,
    })
}

/// The connector's tools, started as a run would.
pub async fn microsoft365_tools(graph: &str) -> Vec<ToolRef> {
    started(&application(graph)).await
}

/// Its tools when the company lets them write (ADR-119).
pub async fn microsoft365_writing_tools(graph: &str) -> Vec<ToolRef> {
    let mut config = application(graph);
    config["writeTools"] = json!(true);
    started(&config).await
}

async fn started(config: &Value) -> Vec<ToolRef> {
    let context = InstanceContext {
        secret: Some("secret-de-l-application"),
        network: LOCAL,
        call_timeout: Duration::from_secs(5),
        ..InstanceContext::default()
    };
    connector_type(MICROSOFT365)
        .unwrap()
        .instantiate(config, context)
        .await
        .unwrap()
        .tools
}

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
