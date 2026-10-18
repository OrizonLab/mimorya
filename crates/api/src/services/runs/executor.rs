//! Executes one run in the background: assembles the agent runtime for the
//! user, starts or resumes it. The runtime records the outcome through its
//! ports; a failure or a panic is recorded as a failed run and never reaches
//! the caller.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use chrono::Utc;
use futures::FutureExt;
use mimorya_agent::llm::{Effort, LlmMessage, ModelRouter, PriceTable};
use mimorya_agent::runtime::{
    AgentRuntime, ApprovalDecisions, BudgetLimits, EnginePorts, RunCheckpoint, RunConfig, RunEvent,
    RuntimeDeps, StartInput, VerificationConfig, build_user_turn,
};
use mimorya_core::authz::Actor;
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use mimorya_db::Database;
use mimorya_db::repositories::runs::RunRow;

use super::context::{build_run_system_prompt, build_tool_registry, recall_memories};
use super::db_ports::{RunScope, engine_ports};
use super::outcome::record_crash;
use crate::services::access::AccessContext;
use crate::services::connectors::ConnectorService;
use crate::services::event_bus::RunEventBus;

const MAX_CORRECTION_ATTEMPTS: u32 = 2;

pub struct RunExecutor {
    pub db: Database,
    pub router: Option<ModelRouter>,
    pub prices: Arc<PriceTable>,
    pub connectors: Arc<ConnectorService>,
    pub bus: Arc<RunEventBus>,
    pub assistant_name: String,
    pub llm_judge: bool,
}

pub enum ExecutionMode {
    Start {
        history: Vec<LlmMessage>,
        text: String,
    },
    Resume {
        /// The saved checkpoint (`agent_runs.state`), decoded by the executor
        /// so that a corrupt state is recorded as a failed run.
        state: serde_json::Value,
        decisions: ApprovalDecisions,
    },
}

impl RunExecutor {
    pub async fn execute(&self, access: Arc<AccessContext>, run: RunRow, mode: ExecutionMode) {
        let scope = RunScope {
            tenant_id: access.subject.tenant_id,
            run_id: run.id,
            user_id: access.subject.user_id,
            conversation_id: run.conversation_id,
        };
        let ports = engine_ports(self.db.clone(), scope, self.bus.clone());
        let attempt = AssertUnwindSafe(self.run(&access, &run, &ports, mode)).catch_unwind();
        let error = match attempt.await {
            Ok(Ok(())) => return,
            Ok(Err(error)) => format!("{error:#}"),
            Err(_) => "panic".to_string(),
        };
        tracing::error!(run_id = %run.id, error, "Run crashed");
        if let Err(error) = record_crash(&self.db, scope).await {
            tracing::error!(run_id = %run.id, error = format!("{error:#}"), "Failed to record run failure");
        }
        let finished = RunEvent::RunFinished {
            status: "failed".into(),
            error: Some("Erreur interne".into()),
        };
        if let Err(error) = ports.events.emit(finished).await {
            tracing::warn!(run_id = %run.id, error = format!("{error:#}"), "Failed to announce run failure");
        }
    }

    async fn run(
        &self,
        access: &AccessContext,
        run: &RunRow,
        ports: &EnginePorts,
        mode: ExecutionMode,
    ) -> anyhow::Result<()> {
        let actor = Actor::agent(Some(run.id)).with_channel("web");
        let runtime = self.runtime_for(access, run, ports, actor.clone()).await?;
        match mode {
            ExecutionMode::Start { history, text } => {
                let system =
                    build_run_system_prompt(&self.db, access, &self.assistant_name, Utc::now())
                        .await?;
                let memories = recall_memories(&self.db, access, &actor, &text).await?;
                runtime
                    .start(StartInput {
                        system,
                        history,
                        user_turn: build_user_turn(&text, &memories),
                    })
                    .await?;
            }
            ExecutionMode::Resume { state, decisions } => {
                let checkpoint: RunCheckpoint = serde_json::from_value(state)?;
                runtime.resume(checkpoint, &decisions).await?;
            }
        }
        Ok(())
    }

    async fn runtime_for(
        &self,
        access: &AccessContext,
        run: &RunRow,
        ports: &EnginePorts,
        actor: Actor,
    ) -> anyhow::Result<AgentRuntime> {
        // A run awaiting approval may be resumed after a restart without a provider.
        let Some(router) = self.router.clone() else {
            anyhow::bail!("no model provider configured");
        };
        let registry = build_tool_registry(access, ports, &self.connectors).await?;
        let budget: BudgetLimits = serde_json::from_value(run.budget.clone())?;
        let level = AutonomyLevel::parse(&run.autonomy_level)
            .ok_or_else(|| anyhow::anyhow!("unknown autonomy level {}", run.autonomy_level))?;
        let deps = RuntimeDeps::new(
            router,
            access.engine.clone(),
            Arc::new(registry),
            ports.clone(),
            self.prices.clone(),
        );
        Ok(AgentRuntime::new(
            deps,
            RunConfig {
                run_id: run.id,
                subject: access.subject.clone(),
                actor,
                objective: run.objective.clone(),
                autonomy: AutonomyPolicy::new(level),
                budget,
                success_criteria: run.success_criteria.clone(),
                verification: VerificationConfig {
                    llm_judge: self.llm_judge,
                    max_correction_attempts: MAX_CORRECTION_ATTEMPTS,
                },
                effort: Some(Effort::High),
            },
        ))
    }
}
