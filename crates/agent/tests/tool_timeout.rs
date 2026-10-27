//! A tool call that runs out of time (ADR-119): stopped, and said to be;
//! one that does not only read may have acted before it was stopped, and
//! the model is told to check before asking it again.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolExecutor, ToolRegistry};
use mimorya_core::authz::Actor;
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy, ToolRisk};
use mimorya_core::demo::{self, user};
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, ToolServices, TypedTool, typed_tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
struct Nothing {}

/// Answers after a second: longer than the executor waits.
struct Slow(ToolRisk);

#[async_trait]
impl TypedTool for Slow {
    type Input = Nothing;

    fn info(&self) -> ToolInfo {
        ToolInfo::new("lent", "Lent", "Répond après une seconde.", self.0)
    }

    async fn run(&self, _input: Nothing, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(ToolResult::ok(json!({ "fait": true })))
    }
}

async fn timed_out(risk: ToolRisk) -> String {
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    let run_id = Uuid::new_v4();
    let registry = ToolRegistry::new(vec![typed_tool(Slow(risk))], &HashMap::new()).unwrap();
    let executor = ToolExecutor::new(
        Arc::new(demo::authorization_engine()),
        Arc::new(registry),
        Arc::new(demo::subject(user::LEA)),
        Actor::agent(Some(run_id)),
        AutonomyPolicy::new(AutonomyLevel::ExecuteWithApproval),
        run_id,
        ToolServices {
            deliverables: ports.ports().deliverables,
            accounts: ports.clone(),
        },
        Duration::from_millis(50),
    );
    let evaluation = executor.evaluate(&LlmToolCall {
        id: "c".into(),
        name: "lent".into(),
        input: json!({}),
    });
    let (ToolCallEvaluation::Allow { tool, input }
    | ToolCallEvaluation::RequireApproval { tool, input, .. }) = evaluation
    else {
        panic!("the call is denied");
    };
    let result = executor.execute(&tool, input).await.0;
    assert!(result.is_error);
    result.content["erreur"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn a_write_out_of_time_may_have_happened() {
    let read = timed_out(ToolRisk::Read).await;
    assert_eq!(read, "Délai d’exécution dépassé");
    for risk in [ToolRisk::Write, ToolRisk::External] {
        let told = timed_out(risk).await;
        assert!(told.contains("a peut-être eu lieu"), "{told}");
    }
}
