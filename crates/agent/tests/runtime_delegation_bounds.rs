//! Runtime: what bounds sub-agents (ADR-098). At most three run per step,
//! side by side, each on an equal share of what is left of its parent's
//! budget; what each spends counts in its parent's, and a sub-agent that
//! does not finish says why. A delegation that needs a validation runs
//! once validated.

mod support;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmError, LlmProvider, LlmRequest, LlmResponse};
use mimorya_agent::runtime::{
    ApprovalDecision, BudgetLimits, DEFAULT_BUDGET, MAX_PER_TURN, RunOutcome, SUB_AGENT_BUDGET,
};
use mimorya_agent::testing::{
    ScriptedCall, ScriptedProvider, ScriptedReply, ScriptedStep, call, last_tool_results,
};
use mimorya_agent::tools::ToolOverride;
use serde_json::json;
use support::{Options, setup};
use tokio::sync::Barrier;

const ROLE: &str = "## Rôle : sous-agent";

fn delegations(count: usize) -> Vec<ScriptedCall> {
    (1..=count)
        .map(|i| ScriptedCall {
            id: Some(format!("d{i}")),
            ..call(
                "delegate_task",
                json!({ "profile": "analysis", "task": format!("Analyse n° {i}") }),
            )
        })
        .collect()
}

/// A sub-agent's step: it answers at once.
fn sub_agent_answers() -> ScriptedStep {
    ScriptedStep::dynamic(|req| {
        assert!(req.system.contains(ROLE));
        ScriptedReply::text("Rien à signaler.")
    })
}

#[tokio::test]
async fn beyond_three_delegations_in_a_step_the_others_wait() {
    let h = setup(
        vec![
            ScriptedStep::calls("", delegations(MAX_PER_TURN + 1)),
            sub_agent_answers(),
            sub_agent_answers(),
            sub_agent_answers(),
            ScriptedStep::dynamic(|req| {
                assert!(!req.system.contains(ROLE));
                let results = last_tool_results(req);
                let ids: Vec<&str> = results.iter().map(|r| r.tool_call_id.as_str()).collect();
                assert_eq!(ids, vec!["d1", "d2", "d3", "d4"]);
                assert!(results[..3].iter().all(|r| !r.is_error));
                assert!(results[3].is_error);
                assert!(
                    results[3]
                        .content
                        .contains("Au plus trois sous-agents par étape")
                );
                ScriptedReply::text("Bilan : rien à signaler.")
            }),
        ],
        Options {
            sub_agents: true,
            ..Options::default()
        },
    );
    assert!(matches!(h.start().await, RunOutcome::Completed { .. }));
    let opened = h.ports.sub_runs();
    assert_eq!(opened.len(), MAX_PER_TURN);
    // Each gets a third of what the parent's first step left.
    let share = BudgetLimits {
        max_steps: SUB_AGENT_BUDGET.max_steps,
        max_input_tokens: (DEFAULT_BUDGET.max_input_tokens - 100) / 3,
        max_output_tokens: (DEFAULT_BUDGET.max_output_tokens - 20) / 3,
        max_cost_micros: SUB_AGENT_BUDGET.max_cost_micros,
        max_duration_ms: opened[0].request.budget.max_duration_ms,
    };
    assert!(share.max_duration_ms <= SUB_AGENT_BUDGET.max_duration_ms);
    assert!(opened.iter().all(|o| o.request.budget == share));
    let refused = h.events_of("tool_call");
    assert_eq!(refused[3]["decision"], "deny");
}

/// The scripted model, each sub-agent's first call held until all three
/// are under way: run one after the other, they would wait forever.
struct Gathered {
    model: ScriptedProvider,
    gathering: Barrier,
}

#[async_trait]
impl LlmProvider for Gathered {
    fn id(&self) -> &str {
        self.model.id()
    }

    fn model(&self) -> &str {
        self.model.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        if request.system.contains(ROLE) {
            self.gathering.wait().await;
        }
        self.model.complete(request).await
    }
}

#[tokio::test]
async fn the_delegations_of_a_step_run_side_by_side() {
    let model = Arc::new(Gathered {
        model: ScriptedProvider::new(vec![
            ScriptedStep::calls("", delegations(MAX_PER_TURN)),
            sub_agent_answers(),
            sub_agent_answers(),
            sub_agent_answers(),
            ScriptedStep::text("Bilan : rien à signaler."),
        ]),
        gathering: Barrier::new(MAX_PER_TURN),
    });
    let h = setup(
        Vec::new(),
        Options {
            sub_agents: true,
            model: Some(model),
            ..Options::default()
        },
    );
    let outcome = tokio::time::timeout(Duration::from_secs(10), h.start())
        .await
        .expect("the sub-agents ran side by side");
    assert!(matches!(outcome, RunOutcome::Completed { .. }));
    assert_eq!(h.ports.sub_runs().len(), MAX_PER_TURN);
}

#[tokio::test]
async fn a_sub_agent_that_does_not_finish_says_why_and_spends_its_parent_s_budget() {
    let budget = BudgetLimits {
        max_output_tokens: 50,
        ..DEFAULT_BUDGET
    };
    let search = || ScriptedStep::calls("", vec![call("jobs_search", json!({ "query": "x" }))]);
    let h = setup(
        // 20 tokens written by the parent, 30 left for its sub-agent, which
        // writes 40 before it stops.
        vec![ScriptedStep::calls("", delegations(1)), search(), search()],
        Options {
            sub_agents: true,
            budget: Some(budget),
            ..Options::default()
        },
    );
    let RunOutcome::Failed { error, checkpoint } = h.start().await else {
        panic!("the parent's budget is spent");
    };
    assert_eq!(error, "Arrêt : budget de tokens atteint");
    assert_eq!(checkpoint.usage.output_tokens, 60);
    let child = &h.ports.sub_runs()[0];
    assert_eq!(child.request.budget.max_output_tokens, 30);
    assert_eq!(child.ports.finished()[0].status, "failed");
    let record = h.ports.tool_records()[0].clone();
    assert_eq!(record.status, "failed");
    assert_eq!(
        record.error.as_deref(),
        Some("Le sous-agent « Analyse » n’a pas abouti : Arrêt : budget de tokens atteint")
    );
}

#[tokio::test]
async fn nothing_left_of_the_budget_opens_no_sub_agent() {
    let budget = BudgetLimits {
        max_input_tokens: 100,
        ..DEFAULT_BUDGET
    };
    let h = setup(
        vec![ScriptedStep::calls("", delegations(1))],
        Options {
            sub_agents: true,
            budget: Some(budget),
            ..Options::default()
        },
    );
    assert!(matches!(h.start().await, RunOutcome::Failed { .. }));
    assert!(h.ports.sub_runs().is_empty());
    let record = h.ports.tool_records()[0].clone();
    assert_eq!(record.status, "failed");
    assert!(
        record
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("Le budget restant ne permet pas"))
    );
}

#[tokio::test]
async fn a_delegation_that_needs_a_validation_runs_once_validated() {
    let overrides = HashMap::from([(
        "delegate_task".to_string(),
        ToolOverride {
            requires_approval: Some(true),
            ..ToolOverride::default()
        },
    )]);
    let h = setup(
        vec![
            ScriptedStep::calls("", delegations(1)),
            sub_agent_answers(),
            ScriptedStep::text("Bilan : rien à signaler."),
        ],
        Options {
            sub_agents: true,
            overrides,
            ..Options::default()
        },
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause");
    };
    assert!(h.ports.sub_runs().is_empty());
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let outcome = h.runtime.resume(checkpoint, &decisions).await.unwrap();
    assert!(matches!(outcome, RunOutcome::Completed { .. }));
    assert_eq!(h.ports.sub_runs().len(), 1);
    let record = h
        .ports
        .tool_records()
        .into_iter()
        .find(|r| r.status == "succeeded")
        .expect("the delegation ran");
    assert_eq!(record.tool_name, "delegate_task");
    assert_eq!(record.approval_id, Some(approvals[0].approval_id));
}
