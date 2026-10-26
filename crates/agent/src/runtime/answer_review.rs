//! Reviews a final answer candidate: deterministic checks first (empty,
//! truncated, ungrounded identifiers), then — when enabled and success
//! criteria exist — a model-based review, on the run's criteria and on the
//! quality criteria of the skills it loaded (ADR-106). A failing reviewer
//! leaves the answer to the next one (ADR-108), and the last never blocks it.

use std::sync::Arc;

use mimorya_core::tools::Citation;
use regex::Regex;

use super::verification::{JudgeInput, VerificationReport, check_grounding, judge_answer};
use crate::llm::{LlmProvider, LlmResponse};
use crate::tools::ToolRegistry;

pub struct AnswerReview<'a> {
    pub answer: &'a str,
    pub truncated: bool,
    pub evidence: &'a [Citation],
    pub registry: &'a ToolRegistry,
    pub objective: &'a str,
    pub success_criteria: &'a [String],
    /// Reviewer models, tried in order until one answers; without any, the
    /// model-based review is skipped.
    pub judges: &'a [Arc<dyn LlmProvider>],
}

/// What the answer is reviewed on: the run's criteria, then, when they
/// apply, those of the skills it loaded that the run's do not already hold.
pub fn review_criteria(explicit: &[String], loaded: &[String], skills_apply: bool) -> Vec<String> {
    let mut criteria = explicit.to_vec();
    if skills_apply {
        for criterion in loaded {
            if !criteria.contains(criterion) {
                criteria.push(criterion.clone());
            }
        }
    }
    criteria
}

/// The report, and the reviewer's response to account for (if a review ran).
pub async fn review_answer(review: AnswerReview<'_>) -> (VerificationReport, Option<LlmResponse>) {
    let mut issues = Vec::new();
    if review.answer.is_empty() {
        issues.push("La réponse est vide.".to_string());
    }
    if review.truncated {
        issues.push("La réponse a été tronquée : fournis une version plus concise.".to_string());
    }
    let patterns: Vec<&Regex> = review
        .registry
        .all()
        .iter()
        .filter_map(|t| t.info.citation_pattern.as_ref())
        .collect();
    issues.extend(check_grounding(review.answer, &patterns, review.evidence));

    let mut judged = false;
    let mut judge_response = None;
    let judges = match issues.is_empty() && !review.success_criteria.is_empty() {
        true => review.judges,
        false => &[],
    };
    for judge in judges {
        let input = JudgeInput {
            objective: review.objective,
            criteria: review.success_criteria,
            answer: review.answer,
            evidence: review.evidence,
        };
        let (verdict, response) = match judge_answer(judge.as_ref(), input).await {
            Ok(judged) => judged,
            Err(error) => {
                // The next one reviews; after the last, the answer is unjudged.
                tracing::warn!(model = judge.model(), error = %error.message, "Answer review failed");
                continue;
            }
        };
        judge_response = Some(response);
        if let Some(verdict) = verdict {
            judged = true;
            if !verdict.pass {
                if verdict.issues.is_empty() {
                    issues.push("Critères non satisfaits.".to_string());
                } else {
                    issues.extend(verdict.issues);
                }
            }
        }
        break;
    }
    (
        VerificationReport {
            passed: issues.is_empty(),
            issues,
            judged,
        },
        judge_response,
    )
}

/// Final text shown to the user: residual issues are never hidden.
pub fn present_answer(answer: &str, report: &VerificationReport) -> String {
    if report.passed {
        return answer.to_string();
    }
    let issues: Vec<String> = report.issues.iter().map(|i| format!("- {i}")).collect();
    format!(
        "{answer}\n\n---\n⚠️ **Points non vérifiés par le contrôle qualité :**\n{}",
        issues.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::review_criteria;

    fn lines(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|t| (*t).to_owned()).collect()
    }

    #[test]
    fn the_skills_criteria_follow_the_runs_each_once_when_they_apply() {
        let explicit = lines(&["Chaque suivi est daté."]);
        let loaded = lines(&["Chaque suivi est daté.", "Cite le technicien."]);
        assert_eq!(
            review_criteria(&explicit, &loaded, true),
            lines(&["Chaque suivi est daté.", "Cite le technicien."])
        );
        assert_eq!(review_criteria(&explicit, &loaded, false), explicit);
        assert!(review_criteria(&[], &[], true).is_empty());
    }
}
