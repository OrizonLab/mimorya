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
    AgentRuntime, ApprovalDecisions, BudgetLimits, EnginePorts, InterruptedCall, RunCheckpoint,
    RunConfig, RunEvent, RuntimeDeps, StartInput, VerificationConfig, build_user_turn,
};
use mimorya_core::authz::Actor;
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use mimorya_db::Database;
use mimorya_db::repositories::runs::{RunRow, RunStatus};

use super::approval_notice::ApprovalNotifier;
use super::context::{build_run_system_prompt, build_run_tools, recall_memories};
use super::db_ports::{RunScope, engine_ports};
use super::outcome::{CRASH_MESSAGE, record_crash};
use super::run_access::RunAccess;
use crate::services::access::{AccessContext, AccessService};
use crate::services::connectors::ConnectorService;
use crate::services::embeddings::EmbeddingService;
use crate::services::event_bus::RunEventBus;
use crate::services::llm::LlmService;

const MAX_CORRECTION_ATTEMPTS: u32 = 2;

pub struct RunExecutor {
    pub db: Database,
    /// Where each step of a run reads its user's rights again (ADR-035).
    pub access: Arc<AccessService>,
    pub llm: Arc<LlmService>,
    pub prices: Arc<PriceTable>,
    /// Searches each company's memory by meaning too (ADR-074).
    pub embeddings: Arc<EmbeddingService>,
    pub connectors: Arc<ConnectorService>,
    pub bus: Arc<RunEventBus>,
    /// Tells a manager a request awaits them (ADR-060).
    pub notifier: ApprovalNotifier,
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
    /// Taken up again after a stop of the service (ADR-053).
    Recover {
        state: serde_json::Value,
        /// Calls the run started since that checkpoint, or the approved
        /// actions it was running.
        interrupted: Vec<InterruptedCall>,
        /// Shown should the engine not even start: the actions to check.
        crash_message: String,
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
        let ports = engine_ports(
            self.db.clone(),
            scope,
            self.bus.clone(),
            self.access.clone(),
            self.notifier.clone(),
            self.embeddings.clone(),
        );
        // A run taken up that crashes says which actions to check.
        let message = match &mode {
            ExecutionMode::Recover { crash_message, .. } => crash_message.clone(),
            _ => CRASH_MESSAGE.to_owned(),
        };
        let attempt = AssertUnwindSafe(self.run(&access, &run, &ports, mode)).catch_unwind();
        let error = match attempt.await {
            Ok(Ok(())) => return,
            Ok(Err(error)) => format!("{error:#}"),
            Err(_) => "panic".to_string(),
        };
        tracing::error!(run_id = %run.id, error, "Run crashed");
        let status = match record_crash(&self.db, scope, &message).await {
            Ok(Some(status)) => status,
            // Its end is already announced (the stale-run sweep failed it).
            Ok(None) => return,
            Err(error) => {
                tracing::error!(run_id = %run.id, error = format!("{error:#}"), "Failed to record run failure");
                RunStatus::Failed
            }
        };
        // Clients following the run learn how it ended, whatever the path.
        let finished = RunEvent::RunFinished {
            status: status.as_str().into(),
            error: (status == RunStatus::Failed).then(|| "Erreur interne".into()),
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
        let (runtime, unavailable_connectors) =
            self.runtime_for(access, run, ports, actor.clone()).await?;
        match mode {
            ExecutionMode::Start { history, text } => {
                let system = build_run_system_prompt(
                    &self.db,
                    access,
                    &actor,
                    &self.assistant_name,
                    Utc::now(),
                )
                .await?;
                let memories =
                    recall_memories(&self.db, &self.embeddings, access, &actor, &text, run.id)
                        .await?;
                runtime
                    .start(StartInput {
                        system,
                        history,
                        user_turn: build_user_turn(&text, &memories),
                        unavailable_connectors,
                    })
                    .await?;
            }
            ExecutionMode::Resume { state, decisions } => {
                let checkpoint: RunCheckpoint = serde_json::from_value(state)?;
                runtime.resume(checkpoint, &decisions).await?;
            }
            ExecutionMode::Recover {
                state, interrupted, ..
            } => {
                let checkpoint: RunCheckpoint = serde_json::from_value(state)?;
                runtime.recover(checkpoint, interrupted).await?;
            }
        }
        Ok(())
    }

    /// The run's runtime, and the connectors in reach of its user that
    /// could not start (told to a run that starts, ADR-048).
    async fn runtime_for(
        &self,
        access: &AccessContext,
        run: &RunRow,
        ports: &EnginePorts,
        actor: Actor,
    ) -> anyhow::Result<(AgentRuntime, Vec<String>)> {
        // A run awaiting approval may be resumed after a restart without a
        // provider, or after its company changed or removed its own.
        let Some(provider) = self.llm.provider_for(access.subject.tenant_id).await? else {
            anyhow::bail!("no model provider configured");
        };
        let router = ModelRouter::new(provider);
        let tools = build_run_tools(access, &actor, ports, &self.connectors).await?;
        let budget: BudgetLimits = serde_json::from_value(run.budget.clone())?;
        let level = AutonomyLevel::parse(&run.autonomy_level)
            .ok_or_else(|| anyhow::anyhow!("unknown autonomy level {}", run.autonomy_level))?;
        let deps = RuntimeDeps::new(
            router,
            Arc::new(RunAccess::new(
                self.access.clone(),
                access.subject.tenant_id,
                access.subject.user_id,
            )),
            Arc::new(tools.registry),
            ports.clone(),
            self.prices.clone(),
        );
        let runtime = AgentRuntime::new(
            deps,
            RunConfig {
                run_id: run.id,
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
        );
        Ok((runtime, tools.unavailable_connectors))
    }
}
