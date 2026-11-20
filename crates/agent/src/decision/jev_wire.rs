//! Jev's System One request and response, as documented on 2026-11-19
//! (<https://docs.typesafe.ai/api/system-one>): `{ model, state, questions }`,
//! each question named, typed and with its criteria; `{ model, answers,
//! usage }`, each answer of its question's type. What is read of a response
//! is checked against the questions by the service.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::answer::DecisionAnswer;
use super::question::{DecisionQuestion, DecisionRequest, QuestionKind};
use crate::llm::LlmUsage;

/// The body asking `request` of `model`.
pub fn body(model: &str, request: &DecisionRequest) -> Value {
    let questions: Map<String, Value> = request
        .questions
        .iter()
        .map(|question| (question.name.clone(), asked(question)))
        .collect();
    json!({
        "model": model,
        "state": request.state.to_json(),
        "questions": questions,
    })
}

fn asked(question: &DecisionQuestion) -> Value {
    match &question.kind {
        QuestionKind::Noul { yes, no } => {
            let mut asked = json!({ "type": "noul", "instructions": question.instructions });
            if yes.is_some() || no.is_some() {
                asked["criteria"] = json!({ "true": yes, "false": no });
            }
            asked
        }
        QuestionKind::Choice { options } => {
            let criteria: Map<String, Value> = options
                .iter()
                .map(|(name, when)| (name.clone(), Value::String(when.clone())))
                .collect();
            json!({ "type": "choice", "instructions": question.instructions, "criteria": criteria })
        }
        QuestionKind::Score { levels } => {
            json!({ "type": "score", "instructions": question.instructions, "criteria": levels })
        }
    }
}

/// What a response answers: the model, each answer read as its type says,
/// and the tokens; `None` when it is not a System One response.
pub fn read(response: &Value) -> Option<(String, BTreeMap<String, DecisionAnswer>, LlmUsage)> {
    let model = response["model"].as_str()?.to_string();
    let answers = response["answers"]
        .as_object()?
        .iter()
        .map(|(name, answer)| Some((name.clone(), answer_of(answer)?)))
        .collect::<Option<BTreeMap<_, _>>>()?;
    let tokens = |key: &str| response["usage"][key].as_u64();
    let usage = LlmUsage {
        input_tokens: tokens("input_tokens")?,
        output_tokens: tokens("output_tokens")?,
        ..LlmUsage::default()
    };
    Some((model, answers, usage))
}

fn answer_of(answer: &Value) -> Option<DecisionAnswer> {
    match answer["type"].as_str()? {
        "noul" => Some(DecisionAnswer::Noul {
            yes: answer["noul"].as_f64()?,
        }),
        "choice" => Some(DecisionAnswer::Choice {
            choice: answer["choice"].as_str()?.to_string(),
            confidence: answer["confidence"].as_f64()?,
            probabilities: answer["probabilities"]
                .as_object()?
                .iter()
                .map(|(name, p)| Some((name.clone(), p.as_f64()?)))
                .collect::<Option<_>>()?,
        }),
        "score" => {
            let by_level = answer["probabilities"].as_object()?;
            // Keyed by level ("0", "1"…): read in their order.
            let mut levels = by_level
                .iter()
                .map(|(level, p)| Some((level.parse::<usize>().ok()?, p.as_f64()?)))
                .collect::<Option<Vec<_>>>()?;
            levels.sort_by_key(|(level, _)| *level);
            let dense = levels.iter().enumerate().all(|(i, (level, _))| i == *level);
            dense.then_some(())?;
            Some(DecisionAnswer::Score {
                score: answer["score"].as_f64()?,
                confidence: answer["confidence"].as_f64()?,
                probabilities: levels.into_iter().map(|(_, p)| p).collect(),
            })
        }
        _ => None,
    }
}
