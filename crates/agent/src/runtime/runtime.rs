//! Agent run loop: call the model, enforce policies on every tool call, pause
//! for human validation, verify the final answer and correct it within
//! bounds. Every step is checkpointed so that a run can be resumed.
//!
//! The loop only orchestrates; the details live next to it:
//!   tool_calls.rs     policy enforcement and execution of tool calls
//!   model_call.rs     retries and usage accounting of model calls
//!   answer_review.rs  verification of the final answer

use std::sync::Arc;
use std::time::Instant;

use serde_json::json;

use super::answer_review::{AnswerReview, present_answer, review_answer};
use super::budget::check_budget;
use super::model_call::{complete_with_retry, record_model_usage};
use super::ports::RunEvent;
use super::state::{PendingCalls, RunCheckpoint};
use super::tool_calls::{ToolCallProcessor, order_by_request};
use super::types::{ApprovalDecisions, RunConfig, RunOutcome, RuntimeDeps};
use super::verification::correction_message;
use crate::llm::{
    LlmErrorKind, LlmMessage, LlmRequest, LlmResponse, LlmToolCall, LlmToolResult, StopReason,
    TaskClass,
};
use crate::tools::{ToolExecutor, to_tool_spec};

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

enum ModelTurn {
    Response(Box<LlmResponse>),
    Failed(String),
}

impl AgentRuntime {
    pub fn new(deps: RuntimeDeps, config: RunConfig) -> Self {
        let executor = ToolExecutor::new(
            deps.engine.clone(),
            deps.registry.clone(),
            config.subject.clone(),
            config.actor.clone(),
            config.autonomy.clone(),
            config.run_id,
            deps.ports.deliverables.clone(),
            deps.tool_timeout,
        );
        let tools = ToolCallProcessor::new(executor, deps.ports.clone());
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

            let response = match self.call_model(&mut state).await? {
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
                if let Some(paused) = self.handle_tool_turn(&mut state, &response).await? {
                    return self.conclude(paused).await;
                }
                continue;
            }

            // Final answer candidate.
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

    /// One model call with the tools visible to the subject; provider failures end the run.
    async fn call_model(&self, state: &mut RunCheckpoint) -> anyhow::Result<ModelTurn> {
        let deps = &self.deps;
        let tools = deps
            .registry
            .visible_to(
                &deps.engine,
                &self.config.subject,
                &self.config.actor,
                &self.config.autonomy,
            )
            .into_iter()
            .map(to_tool_spec)
            .collect();
        let events = deps.ports.events.clone();
        let request = LlmRequest {
            system: state.system.clone(),
            messages: state.messages.clone(),
            tools,
            max_output_tokens: None,
            effort: self.config.effort,
            on_text_delta: Some(Arc::new(move |text: &str| events.publish_delta(text))),
        };
        let provider = deps.router.provider_for(TaskClass::Agent);
        match complete_with_retry(
            provider.as_ref(),
            &request,
            deps.max_provider_retries,
            deps.retry_delay,
        )
        .await
        {
            Ok(response) => {
                state.usage.steps += 1;
                record_model_usage(&deps.ports, &deps.prices, state, &response).await?;
                Ok(ModelTurn::Response(Box::new(response)))
            }
            Err(error) if error.kind == LlmErrorKind::Aborted => {
                Ok(ModelTurn::Failed("Exécution annulée".into()))
            }
            Err(error) => Ok(ModelTurn::Failed(error.message)),
        }
    }

    /// Returns an outcome when the run must pause for validation.
    async fn handle_tool_turn(
        &self,
        state: &mut RunCheckpoint,
        response: &LlmResponse,
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

        let turn = self.tools.process_turn(state, &response.tool_calls).await?;
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
