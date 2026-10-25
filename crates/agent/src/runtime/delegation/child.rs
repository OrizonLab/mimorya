//! One sub-agent's run (ADR-098): opened beside its parent, executed to its
//! end with what a sub-agent is given, and told back to the parent as a
//! tool result: its summary and the sources it read, or why it did not
//! finish.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use mimorya_core::authz::Actor;
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use mimorya_core::tools::ToolResult;
use serde_json::json;
use uuid::Uuid;

use super::prompt::{sub_agent_system, sub_agent_turn};
use super::store::{SubRunRequest, SubRunStore};
use crate::llm::{Effort, ModelRouter, PriceTable};
use crate::runtime::budget::BudgetLimits;
use crate::runtime::ports::AccessSource;
use crate::runtime::runtime::{AgentRuntime, StartInput};
use crate::runtime::state::RunUsageTotals;
use crate::runtime::types::{RunConfig, RunOutcome, RuntimeDeps, VerificationConfig};
use crate::tools::ToolRegistry;
use crate::tools::platform::{DelegateTaskInput, platform_tools};

/// The sources told back to the parent, at most.
const MAX_SOURCES: usize = 30;

/// What a sub-agent works with: its parent's model, rights and settings,
/// and the tools a sub-agent may be given, the platform's run through its
/// own ports.
pub struct SubAgents {
    router: ModelRouter,
    access: Arc<dyn AccessSource>,
    registry: Arc<ToolRegistry>,
    prices: Arc<PriceTable>,
    max_provider_retries: u32,
    retry_delay: Duration,
    tool_timeout: Duration,
    store: Arc<dyn SubRunStore>,
    /// The parent's: its ceiling is the child's.
    actor: Actor,
    effort: Option<Effort>,
}

/// What a sub-agent's run came to, for its parent.
pub struct SubAgentReport {
    /// The child run, when one was opened.
    pub run_id: Option<Uuid>,
    pub result: ToolResult,
    /// What the child spent, counted in its parent's budget.
    pub usage: RunUsageTotals,
    pub duration_ms: u64,
}

impl SubAgents {
    pub fn new(deps: &RuntimeDeps, config: &RunConfig) -> Self {
        Self {
            router: deps.router.clone(),
            access: deps.access.clone(),
            registry: Arc::new(deps.registry.for_sub_agents()),
            prices: deps.prices.clone(),
            max_provider_retries: deps.max_provider_retries,
            retry_delay: deps.retry_delay,
            tool_timeout: deps.tool_timeout,
            store: deps.ports.sub_runs.clone(),
            actor: config.actor.clone(),
            effort: config.effort,
        }
    }

    /// Runs `input`'s task in a child run within `budget`, `None` when
    /// nothing is left of the parent's to share.
    pub async fn run(
        &self,
        parent_system: &str,
        input: &DelegateTaskInput,
        budget: Option<BudgetLimits>,
    ) -> anyhow::Result<SubAgentReport> {
        let started = Instant::now();
        let label = input.profile.label();
        let Some(budget) = budget else {
            let result = ToolResult::error(format!(
                "Le budget restant ne permet pas de confier cette tâche au sous-agent « {label} »."
            ))
            .with_summary(format!("Sous-agent « {label} » : budget épuisé"));
            return Ok(SubAgentReport {
                run_id: None,
                result,
                usage: RunUsageTotals::default(),
                duration_ms: 0,
            });
        };
        let allowed = self.actor.allowed_permissions.as_ref().map(|set| {
            let mut permissions: Vec<String> = set.iter().cloned().collect();
            permissions.sort();
            permissions
        });
        let sub_run = self
            .store
            .open(SubRunRequest {
                profile: input.profile,
                objective: input.task.clone(),
                budget,
                allowed_permissions: allowed,
            })
            .await?;
        let ports = &sub_run.ports;
        let own = platform_tools(
            ports.memory.clone(),
            ports.skills.clone(),
            ports.documents.clone(),
        );
        let mut deps = RuntimeDeps::new(
            self.router.clone(),
            self.access.clone(),
            Arc::new(self.registry.rebound(own)),
            sub_run.ports,
            self.prices.clone(),
        );
        deps.max_provider_retries = self.max_provider_retries;
        deps.retry_delay = self.retry_delay;
        deps.tool_timeout = self.tool_timeout;
        let runtime = AgentRuntime::new(deps, self.config(sub_run.run_id, input, budget));
        let start = StartInput {
            system: sub_agent_system(parent_system, input.profile),
            history: Vec::new(),
            user_turn: sub_agent_turn(&input.task, input.context.as_deref()),
            unavailable_connectors: Vec::new(),
            evidence: Vec::new(),
        };
        let outcome = started_apart(&runtime, start).await?;
        Ok(SubAgentReport {
            run_id: Some(sub_run.run_id),
            result: told(input, &outcome),
            usage: outcome.checkpoint().usage,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn config(&self, run_id: Uuid, input: &DelegateTaskInput, budget: BudgetLimits) -> RunConfig {
        let mut actor = Actor::agent(Some(run_id));
        actor.allowed_permissions = self.actor.allowed_permissions.clone();
        actor.channel = self.actor.channel.clone();
        RunConfig {
            run_id,
            actor,
            objective: input.task.clone(),
            autonomy: AutonomyPolicy::new(AutonomyLevel::ReadOnly),
            budget,
            success_criteria: Vec::new(),
            verification: VerificationConfig {
                llm_judge: false,
                max_correction_attempts: 1,
            },
            effort: self.effort,
            handoff: false,
        }
    }
}

/// The child's run, behind a box: a run's future cannot contain itself.
fn started_apart(
    runtime: &AgentRuntime,
    start: StartInput,
) -> BoxFuture<'_, anyhow::Result<RunOutcome>> {
    Box::pin(runtime.start(start))
}

/// The child's outcome as its parent reads it.
fn told(input: &DelegateTaskInput, outcome: &RunOutcome) -> ToolResult {
    let label = input.profile.label();
    match outcome {
        RunOutcome::Completed {
            answer, checkpoint, ..
        } => {
            let sources: Vec<_> = checkpoint.evidence.iter().take(MAX_SOURCES).collect();
            ToolResult::ok(json!({
                "profil": input.profile.as_str(),
                "resume": answer,
                "sources": sources,
            }))
            .with_summary(format!("Sous-agent « {label} » : résumé rendu"))
            .with_citations(checkpoint.evidence.clone())
        }
        RunOutcome::Failed { error, .. } => ToolResult::error(format!(
            "Le sous-agent « {label} » n’a pas abouti : {error}"
        ))
        .with_summary(format!("Sous-agent « {label} » : échec")),
        RunOutcome::AwaitingApproval { .. } | RunOutcome::Handoff { .. } => ToolResult::error(
            format!("Le sous-agent « {label} » s’est arrêté sans rendre de résumé."),
        )
        .with_summary(format!("Sous-agent « {label} » : échec")),
    }
}
