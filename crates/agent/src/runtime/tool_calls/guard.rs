//! A call's evaluation once the run's suspect content (ADR-213, D6) and the
//! company's guard of the actions read it (D5): asked only of a call with a side effect the policy
//! enforcement point allows or puts to validation, before either; what its
//! decision cost counted in the run.

use super::ToolCallProcessor;
use crate::runtime::action_guard::{GuardedCall, guarded, guarded_evaluation};
use crate::runtime::content_screen::after_suspect_content;
use crate::runtime::decision_costs::count;
use crate::runtime::state::RunCheckpoint;
use crate::tools::ToolCallEvaluation;

impl ToolCallProcessor {
    pub(super) async fn guard(
        &self,
        state: &mut RunCheckpoint,
        evaluation: ToolCallEvaluation,
    ) -> anyhow::Result<ToolCallEvaluation> {
        // A run that read a suspect external content puts each action to a
        // person (ADR-213, D6), whatever the guard says next.
        let evaluation = if state.external_suspect {
            after_suspect_content(evaluation)
        } else {
            evaluation
        };
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
