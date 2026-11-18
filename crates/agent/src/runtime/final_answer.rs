//! The review of a run's answer candidate: deterministic checks, then a
//! model's on the run's criteria and those of the skills it loaded
//! (ADR-106), by the company's review model if it chose one (ADR-108). The
//! reviewer's call is accounted as a review; the verdict is told to the
//! run's listeners.

use super::answer_review::{AnswerReview, review_answer, review_criteria};
use super::model_call::{ModelCall, record_model_usage};
use super::ports::RunEvent;
use super::runtime::AgentRuntime;
use super::state::RunCheckpoint;
use super::verification::VerificationReport;
use crate::llm::{LlmResponse, ModelRole, StopReason, TaskClass};

impl AgentRuntime {
    /// The answer of `response`, trimmed, and what its review found.
    pub(super) async fn review_candidate(
        &self,
        state: &mut RunCheckpoint,
        response: &LlmResponse,
    ) -> anyhow::Result<(String, VerificationReport)> {
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
        let judges = match judged {
            true => self.deps.router.providers_for(TaskClass::Verification),
            false => Vec::new(),
        };
        let (report, judge_response) = review_answer(AnswerReview {
            answer: &answer,
            truncated: response.stop_reason == StopReason::MaxTokens,
            evidence: &state.evidence,
            registry: &self.deps.registry,
            objective: &self.config.objective,
            success_criteria: &criteria,
            judges: &judges,
        })
        .await;
        if let Some((judge_response, requested)) = judge_response {
            let call = ModelCall {
                role: ModelRole::Review,
                requested: &requested,
            };
            record_model_usage(
                &self.deps.ports,
                &self.deps.prices,
                state,
                &judge_response,
                call,
            )
            .await?;
        }
        self.deps
            .ports
            .events
            .emit(RunEvent::Verification {
                // The escalated answer is an attempt of its own (ADR-207).
                attempt: state.correction_attempts
                    + 1
                    + u32::from(state.escalation.after_failure_used),
                passed: report.passed,
                issues: report.issues.clone(),
            })
            .await?;
        Ok((answer, report))
    }
}
