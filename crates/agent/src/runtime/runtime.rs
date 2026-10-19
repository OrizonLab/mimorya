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

use std::time::Instant;

use serde_json::json;

use super::answer_review::{AnswerReview, present_answer, review_answer};
use super::budget::check_budget;
use super::model_call::{ModelTurn, call_model, record_model_usage};
use super::ports::{CurrentAccess, RunEvent, StepAccess};
use super::state::{PendingCalls, RunCheckpoint};
use super::tool_calls::{ToolCallProcessor, order_by_request};
use super::types::{ApprovalDecisions, RunConfig, RunOutcome, RuntimeDeps};
use super::verification::correction_message;
use crate::llm::{LlmMessage, LlmResponse, LlmToolCall, LlmToolResult, StopReason, TaskClass};

/// Why a run stops when its user's account is no longer active.
pub const ACCOUNT_INACTIVE: &str = "Arrêt : le compte de l’utilisateur n’est plus actif";

pub struct StartInput {
    pub system: String,
    /// Previous turns of the conversation (text only).
    pub history: Vec<LlmMessage>,
    pub user_turn: String,
}

pub struct AgentRuntime {
    deps: RuntimeDeps,
    config: RunConfig,
    tools: ToolCallProcessor,
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
        messages.push(LlmMessage::user(input.user_turn));
        let state = RunCheckpoint::new(input.system, messages);
        self.deps
            .ports
            .events
            .emit(RunEvent::RunStarted {
                objective: self.config.objective.clone(),
            })
            .await?;
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
            let result = match (self.tools.tool(&approval.tool_name), decision.approved) {
                (None, _) => {
                    self.tools
                        .reject(
                            approval,
                            "Outil désactivé depuis la demande de validation".into(),
                        )
                        .await?
                }
                (Some(_), false) => {
                    let comment = decision
                        .comment
                        .as_ref()
                        .map(|c| format!(" : {c}"))
                        .unwrap_or_default();
                    self.tools
                        .reject(
                            approval,
                            format!("Action refusée par l’utilisateur{comment}"),
                        )
                        .await?
                }
                (Some(tool), true) => {
                    self.tools
                        .run(
                            &mut state,
                            &tool,
                            &approval.tool_call_id,
                            approval.input.clone(),
                            Some(approval.approval_id),
                            &access,
                        )
                        .await?
                }
            };
            results.push(result);
        }

        state.pending = None;
        let results = order_by_request(&state, results);
        state.messages.push(LlmMessage::ToolResults { results });
        self.deps.ports.runs.checkpoint(&state).await?;
        self.run_loop(state).await
    }

    async fn run_loop(&self, mut state: RunCheckpoint) -> anyhow::Result<RunOutcome> {
        let started = Instant::now();
        let elapsed_before = state.elapsed_ms;
        let elapsed = |started: &Instant| elapsed_before + started.elapsed().as_millis() as u64;

        loop {
            state.elapsed_ms = elapsed(&started);
            if let Err(reason) = check_budget(&self.config.budget, &state.usage, state.elapsed_ms) {
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
            let judge = self
                .config
                .verification
                .llm_judge
                .then(|| self.deps.router.provider_for(TaskClass::Verification));
            let (report, judge_response) = review_answer(AnswerReview {
                answer: &answer,
                truncated: response.stop_reason == StopReason::MaxTokens,
                evidence: &state.evidence,
                registry: &self.deps.registry,
                objective: &self.config.objective,
                success_criteria: &self.config.success_criteria,
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
    async fn end_without_access(
        &self,
        checkpoint: RunCheckpoint,
        error: String,
    ) -> anyhow::Result<RunOutcome> {
        self.conclude(RunOutcome::Failed { error, checkpoint })
            .await
    }

    /// Records the outcome, then announces it: listeners must find it saved.
    async fn conclude(&self, outcome: RunOutcome) -> anyhow::Result<RunOutcome> {
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
