//! The review of a run's answer candidate: deterministic checks, then a
//! model's on the run's criteria and those of the skills it loaded
//! (ADR-106), by the company's review model if it chose one, else or should
//! it fail by the run's own (ADR-108), each only while the operator's supply
//! allows its call (ADR-208). The reviewer's call is accounted as a review,
//! on the key of the model that made it; the verdict is told to the run's
//! listeners.

use super::answer_review::{AnswerReview, Judge, review_answer, review_criteria};
use super::model_call::{ModelCall, record_model_usage};
use super::ports::RunEvent;
use super::runtime::AgentRuntime;
use super::state::RunCheckpoint;
use super::supply::supply_stopped;
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
            true => self.judges(state).await?,
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
        if let Some((judge_response, judge)) = judge_response {
            let call = ModelCall {
                role: ModelRole::Review,
                by_runs_model: judge.runs_model,
                requested: judge.provider.model(),
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

    /// The models that may judge the answer, in order: the company's review
    /// model, then the run's own (ADR-108), each only while the operator's
    /// supply allows its call (ADR-208). Past it, the answer is left
    /// unjudged; a review model on the company's own key is never held back.
    async fn judges(&self, state: &mut RunCheckpoint) -> anyhow::Result<Vec<Judge>> {
        let mut judges = Vec::new();
        if let Some(reviewer) = self.deps.router.reviewer()
            && supply_stopped(&self.deps, state, ModelRole::Review)
                .await?
                .is_none()
        {
            judges.push(Judge {
                provider: reviewer,
                runs_model: false,
            });
        }
        if supply_stopped(&self.deps, state, ModelRole::Main)
            .await?
            .is_none()
        {
            judges.push(Judge {
                provider: self.deps.router.provider_for(TaskClass::Agent),
                runs_model: true,
            });
        }
        Ok(judges)
    }
}
