//! Agent run loop: call the model, enforce policies on every tool call, pause
//! for human validation, verify the final answer and correct it within
//! bounds. Every step is checkpointed so that a run can be resumed. The
//! user's rights are read again before each model call, before the tool
//! calls it returns, before concluding and when resuming (ADR-035): a run
//! its user may no longer use ends there, a right withdrawn no longer counts.
//!
//! The loop only orchestrates; the details live next to it:
//!   tool_calls.rs     policy enforcement and execution of tool calls
//!   model_call.rs     model calls, their retries and usage accounting
//!   answer_review.rs  verification of the final answer
//!   handoff.rs        the progress note ending a mission's stage

use std::time::Instant;

use mimorya_core::tools::Citation;
use serde_json::json;

use super::answer_review::{AnswerReview, present_answer, review_answer, review_criteria};
use super::budget::check_budget;
use super::model_call::{ModelTurn, call_model, record_model_usage};
use super::ports::{CurrentAccess, RunEvent, StepAccess};
use super::state::{PendingCalls, RunCheckpoint};
use super::tool_calls::{ToolCallProcessor, order_by_request};
use super::tool_refusals::why_not_run;
use super::types::{ApprovalDecisions, RunConfig, RunOutcome, RuntimeDeps};
use super::unavailable::with_unavailable_connectors;
use super::verification::correction_message;
use crate::llm::{LlmMessage, LlmResponse, LlmToolCall, LlmToolResult, StopReason, TaskClass};

/// Why a run stops when its user's account is no longer active.
pub const ACCOUNT_INACTIVE: &str = "Arrêt : le compte de l’utilisateur n’est plus actif";

pub struct StartInput {
    pub system: String,
    /// Previous turns of the conversation (text only).
    pub history: Vec<LlmMessage>,
    pub user_turn: String,
    /// Connectors in reach of the user that could not start (ADR-048).
    pub unavailable_connectors: Vec<String>,
    /// Sources a mission's earlier stages collected (ADR-089): an answer
    /// may cite them.
    pub evidence: Vec<Citation>,
}

pub struct AgentRuntime {
    pub(super) deps: RuntimeDeps,
    pub(super) config: RunConfig,
    pub(super) tools: ToolCallProcessor,
}

impl AgentRuntime {
    pub fn new(deps: RuntimeDeps, config: RunConfig) -> Self {
        let tools = ToolCallProcessor::new(&deps, &config);
        Self {
            deps,
            config,
            tools,
        }
    }

    pub async fn start(&self, input: StartInput) -> anyhow::Result<RunOutcome> {
        let mut messages = input.history;
        let connectors = input.unavailable_connectors;
        let turn = with_unavailable_connectors(&connectors, input.user_turn);
        messages.push(LlmMessage::user(turn));
        let mut state = RunCheckpoint::new(input.system, messages);
        state.evidence = input.evidence;
        let events = &self.deps.ports.events;
        let objective = self.config.objective.clone();
        events.emit(RunEvent::RunStarted { objective }).await?;
        if !connectors.is_empty() {
            events
                .emit(RunEvent::ConnectorsUnavailable { connectors })
                .await?;
        }
        // Saved before the first model call: a run the service stops from
        // here on can be taken up again (ADR-053).
        self.deps.ports.runs.checkpoint(&state).await?;
        self.run_loop(state).await
    }

    /// Continues a run paused for validation. Approved calls run with the
    /// exact parameters that were validated; rejected ones are reported to
    /// the model.
    pub async fn resume(
        &self,
        checkpoint: RunCheckpoint,
        decisions: &ApprovalDecisions,
    ) -> anyhow::Result<RunOutcome> {
        let Some(pending) = checkpoint.pending.clone() else {
            anyhow::bail!("Run is not waiting for approval");
        };
        if pending
            .approvals
            .iter()
            .any(|a| !decisions.contains_key(&a.approval_id))
        {
            return Ok(RunOutcome::AwaitingApproval {
                approvals: pending.approvals,
                checkpoint,
            });
        }

        let access = match self.deps.access.current().await? {
            StepAccess::Granted(access) => access,
            StepAccess::Ended(reason) => {
                // Every pending call gets its outcome in the log: not run.
                let not_run = format!("Non exécuté — {reason}");
                for approval in &pending.approvals {
                    self.tools
                        .abandon(approval, &decisions[&approval.approval_id], &not_run)
                        .await?;
                }
                return self.end_without_access(checkpoint, reason).await;
            }
        };
        let mut state = checkpoint;
        let mut results = pending.completed_results;
        let mut validated = Vec::new();
        for approval in &pending.approvals {
            let decision = &decisions[&approval.approval_id];
            self.deps
                .ports
                .events
                .emit(RunEvent::ApprovalResolved {
                    approval_id: approval.approval_id,
                    approved: decision.approved,
                })
                .await?;
            match self.tools.tool(&approval.tool_name) {
                Some(tool) if decision.approved => validated.push((tool, approval)),
                tool => {
                    let (reason, shown) = why_not_run(tool.is_some(), decision);
                    results.push(self.tools.reject(approval, reason, shown).await?);
                }
            }
        }
        // The validated calls' time counts in the run's, its sub-agents'
        // included (ADR-098); the wait for a decision does not.
        let started = Instant::now();
        let ran = self.tools.run_validated(&mut state, validated, &access);
        results.extend(ran.await?);
        state.elapsed_ms += started.elapsed().as_millis() as u64;

        state.pending = None;
        let results = order_by_request(&state, results);
        state.messages.push(LlmMessage::ToolResults { results });
        self.deps.ports.runs.checkpoint(&state).await?;
        self.run_loop(state).await
    }

    pub(super) async fn run_loop(&self, mut state: RunCheckpoint) -> anyhow::Result<RunOutcome> {
        let started = Instant::now();
        let elapsed_before = state.elapsed_ms;
        let elapsed = |started: &Instant| elapsed_before + started.elapsed().as_millis() as u64;

        loop {
            state.elapsed_ms = elapsed(&started);
            if let Err(reason) = check_budget(&self.config.budget, &state.usage, state.elapsed_ms) {
                if self.config.handoff {
                    return self.hand_over(state, &reason).await;
                }
                return self
                    .conclude(RunOutcome::Failed {
                        error: format!("Arrêt : {reason}"),
                        checkpoint: state,
                    })
                    .await;
            }

            let access = match self.deps.access.current().await? {
                StepAccess::Granted(access) => access,
                StepAccess::Ended(reason) => return self.end_without_access(state, reason).await,
            };

            let response = match call_model(&self.deps, &self.config, &mut state, &access).await? {
                ModelTurn::Response(response) => response,
                ModelTurn::Failed(error) => {
                    return self
                        .conclude(RunOutcome::Failed {
                            error,
                            checkpoint: state,
                        })
                        .await;
                }
            };
            state.messages.push(LlmMessage::Assistant {
                content: response.text.clone(),
                tool_calls: response.tool_calls.clone(),
                native: response.native.clone(),
            });

            if response.stop_reason == StopReason::Refusal {
                let error = "Le modèle a refusé de traiter cette demande.".to_string();
                return self
                    .conclude(RunOutcome::Failed {
                        error,
                        checkpoint: state,
                    })
                    .await;
            }

            if !response.tool_calls.is_empty() {
                // What its sub-agents may take of the time left counts the
                // model's call (ADR-098).
                state.elapsed_ms = elapsed(&started);
                // The model may have taken a while: its calls are judged with
                // the rights as they stand now.
                let access = match self.deps.access.current().await? {
                    StepAccess::Granted(access) => access,
                    StepAccess::Ended(reason) => {
                        return self.end_without_access(state, reason).await;
                    }
                };
                if let Some(paused) = self
                    .handle_tool_turn(&mut state, &response, &access)
                    .await?
                {
                    return self.conclude(paused).await;
                }
                continue;
            }

            // Final answer candidate: reviewed (possibly by a model) and
            // concluded only while the user may still use the agent.
            if let StepAccess::Ended(reason) = self.deps.access.current().await? {
                return self.end_without_access(state, reason).await;
            }
            let answer = response.text.trim().to_string();
            // The criteria of a skill loaded are reviewed by a model, as
            // explicit ones are when the platform asks for it (ADR-106).
            let verification = self.config.verification;
            let criteria = review_criteria(
                &self.config.success_criteria,
                &state.skill_criteria,
                verification.skill_criteria,
            );
            let judged = verification.llm_judge
                || (verification.skill_criteria && !state.skill_criteria.is_empty());
            let judge = judged.then(|| self.deps.router.provider_for(TaskClass::Verification));
            let (report, judge_response) = review_answer(AnswerReview {
                answer: &answer,
                truncated: response.stop_reason == StopReason::MaxTokens,
                evidence: &state.evidence,
                registry: &self.deps.registry,
                objective: &self.config.objective,
                success_criteria: &criteria,
                judge: judge.as_deref(),
            })
            .await;
            if let Some(judge_response) = judge_response {
                record_model_usage(
                    &self.deps.ports,
                    &self.deps.prices,
                    &mut state,
                    &judge_response,
                )
                .await?;
            }
            self.deps
                .ports
                .events
                .emit(RunEvent::Verification {
                    attempt: state.correction_attempts + 1,
                    passed: report.passed,
                    issues: report.issues.clone(),
                })
                .await?;
            let can_correct =
                state.correction_attempts < self.config.verification.max_correction_attempts;
            if !report.passed && can_correct {
                state.correction_attempts += 1;
                state
                    .messages
                    .push(LlmMessage::user(correction_message(&report.issues)));
                self.deps.ports.runs.checkpoint(&state).await?;
                continue;
            }

            let final_answer = present_answer(&answer, &report);
            state.elapsed_ms = elapsed(&started);
            self.deps
                .ports
                .events
                .emit(RunEvent::AssistantMessage {
                    text: final_answer.clone(),
                    is_final: true,
                })
                .await?;
            return self
                .conclude(RunOutcome::Completed {
                    answer: final_answer,
                    verification: report,
                    checkpoint: state,
                })
                .await;
        }
    }

    /// Returns an outcome when the run must pause for validation.
    async fn handle_tool_turn(
        &self,
        state: &mut RunCheckpoint,
        response: &LlmResponse,
        access: &CurrentAccess,
    ) -> anyhow::Result<Option<RunOutcome>> {
        let ports = &self.deps.ports;
        if !response.text.is_empty() {
            ports
                .events
                .emit(RunEvent::AssistantMessage {
                    text: response.text.clone(),
                    is_final: false,
                })
                .await?;
        }
        if response.stop_reason == StopReason::MaxTokens {
            // A truncated tool input must never run: report it and let the model retry.
            state.messages.push(LlmMessage::ToolResults {
                results: truncated_results(&response.tool_calls),
            });
            ports.runs.checkpoint(state).await?;
            return Ok(None);
        }

        let turn = self
            .tools
            .process_turn(state, &response.tool_calls, access)
            .await?;
        if !turn.awaiting.is_empty() {
            state.pending = Some(PendingCalls {
                completed_results: turn.results,
                approvals: turn.awaiting.clone(),
            });
            return Ok(Some(RunOutcome::AwaitingApproval {
                approvals: turn.awaiting,
                checkpoint: state.clone(),
            }));
        }
        state.messages.push(LlmMessage::ToolResults {
            results: turn.results,
        });
        ports.runs.checkpoint(state).await?;
        Ok(None)
    }

    /// Ends the run when its user may no longer act through it.
    pub(super) async fn end_without_access(
        &self,
        checkpoint: RunCheckpoint,
        error: String,
    ) -> anyhow::Result<RunOutcome> {
        self.conclude(RunOutcome::Failed { error, checkpoint })
            .await
    }

    /// Records the outcome, then announces it: listeners must find it saved.
    pub(super) async fn conclude(&self, outcome: RunOutcome) -> anyhow::Result<RunOutcome> {
        let ports = &self.deps.ports;
        ports.runs.finish(&outcome).await?;
        let error = match &outcome {
            RunOutcome::Failed { error, .. } => Some(error.clone()),
            _ => None,
        };
        ports
            .events
            .emit(RunEvent::RunFinished {
                status: outcome.status().into(),
                error,
            })
            .await?;
        Ok(outcome)
    }
}

fn truncated_results(calls: &[LlmToolCall]) -> Vec<LlmToolResult> {
    let content = json!({ "erreur": "Appel tronqué (limite de longueur atteinte), non exécuté." })
        .to_string();
    calls
        .iter()
        .map(|c| LlmToolResult {
            tool_call_id: c.id.clone(),
            content: content.clone(),
            is_error: true,
        })
        .collect()
}
