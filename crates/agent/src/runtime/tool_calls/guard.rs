//! A call's evaluation once the company's guard of the actions read it
//! (ADR-213, D5): asked only of a call with a side effect the policy
//! enforcement point allows or puts to validation, before either; what its
//! decision cost counted in the run.

use super::ToolCallProcessor;
use crate::runtime::action_guard::{GuardedCall, guarded, guarded_evaluation};
use crate::runtime::decision_costs::count;
use crate::runtime::state::RunCheckpoint;
use crate::tools::ToolCallEvaluation;

impl ToolCallProcessor {
    pub(super) async fn guard(
        &self,
        state: &mut RunCheckpoint,
        evaluation: ToolCallEvaluation,
    ) -> anyhow::Result<ToolCallEvaluation> {
        let review = {
            let (Some(tool), Some(input)) = (guarded(&evaluation), input_of(&evaluation)) else {
                return Ok(evaluation);
            };
            let action = tool.tool.describe_action(input);
            let call = GuardedCall {
                tool: &tool.info.name,
                title: &tool.info.title,
                action: action.as_deref(),
                input,
            };
            self.ports.action_guard.review(&call).await
        };
        count(&self.ports, state, &review.counted).await?;
        Ok(guarded_evaluation(evaluation, &review.verdict))
    }
}

fn input_of(evaluation: &ToolCallEvaluation) -> Option<&serde_json::Value> {
    match evaluation {
        ToolCallEvaluation::Allow { input, .. }
        | ToolCallEvaluation::RequireApproval { input, .. } => Some(input),
        ToolCallEvaluation::Deny { .. } => None,
    }
}
