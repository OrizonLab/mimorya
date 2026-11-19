//! What a decision answers (ADR-213): for each question, its typed answer,
//! checked against what was asked before anything reads it — one answer a
//! question, of its type, finite probabilities between 0 and 1, an option
//! or a level that exists. Anything else is an invalid answer.

use std::collections::BTreeMap;

use serde::Serialize;

use super::question::{DecisionQuestion, QuestionKind};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DecisionAnswer {
    /// The probability of yes.
    Noul { yes: f64 },
    Choice {
        choice: String,
        confidence: f64,
        probabilities: BTreeMap<String, f64>,
    },
    /// The expected level (from 0), each level's probability.
    Score {
        score: f64,
        confidence: f64,
        probabilities: Vec<f64>,
    },
}

fn unit(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

impl DecisionAnswer {
    /// Whether it answers `question` as it was asked.
    fn answers(&self, question: &DecisionQuestion) -> bool {
        match (self, &question.kind) {
            (DecisionAnswer::Noul { yes }, QuestionKind::Noul { .. }) => unit(*yes),
            (
                DecisionAnswer::Choice {
                    choice,
                    confidence,
                    probabilities,
                },
                QuestionKind::Choice { options },
            ) => {
                options.iter().any(|(name, _)| name == choice)
                    && unit(*confidence)
                    && probabilities.values().all(|p| unit(*p))
                    && probabilities
                        .keys()
                        .all(|key| options.iter().any(|(name, _)| name == key))
            }
            (
                DecisionAnswer::Score {
                    score,
                    confidence,
                    probabilities,
                },
                QuestionKind::Score { levels },
            ) => {
                let top = (levels.len() - 1) as f64;
                score.is_finite()
                    && (0.0..=top).contains(score)
                    && unit(*confidence)
                    && probabilities.len() <= levels.len()
                    && probabilities.iter().all(|p| unit(*p))
            }
            _ => false,
        }
    }
}

/// The answers, if they are exactly those asked: one for each question,
/// none other.
pub fn checked(
    questions: &[DecisionQuestion],
    answers: BTreeMap<String, DecisionAnswer>,
) -> Option<BTreeMap<String, DecisionAnswer>> {
    let exact = answers.len() == questions.len()
        && questions.iter().all(|question| {
            answers
                .get(&question.name)
                .is_some_and(|answer| answer.answers(question))
        });
    exact.then_some(answers)
}

/// The engine that answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Jev,
    MainModel,
}

impl EngineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EngineKind::Jev => "jev",
            EngineKind::MainModel => "main_model",
        }
    }
}

/// A decision made: by which engine and model, how fast, and its answers.
#[derive(Debug, Clone, PartialEq)]
pub struct Decided {
    pub engine: EngineKind,
    pub model: String,
    pub latency_ms: u64,
    pub answers: BTreeMap<String, DecisionAnswer>,
}

impl Decided {
    /// The probability of yes to `question`, if it was a yes or no.
    pub fn yes(&self, question: &str) -> Option<f64> {
        match self.answers.get(question)? {
            DecisionAnswer::Noul { yes } => Some(*yes),
            _ => None,
        }
    }

    /// The option chosen for `question`, and the confidence in it.
    pub fn choice(&self, question: &str) -> Option<(&str, f64)> {
        match self.answers.get(question)? {
            DecisionAnswer::Choice {
                choice, confidence, ..
            } => Some((choice.as_str(), *confidence)),
            _ => None,
        }
    }

    /// The level scored for `question`, and the confidence in it.
    pub fn score(&self, question: &str) -> Option<(f64, f64)> {
        match self.answers.get(question)? {
            DecisionAnswer::Score {
                score, confidence, ..
            } => Some((*score, *confidence)),
            _ => None,
        }
    }
}
