//! Deterministic provider for tests: each call consumes the next scripted
//! step. A step may inspect the request (visible tools, last tool results) to
//! decide its answer, which lets tests assert on what the model was shown.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;

use crate::llm::{
    LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall, LlmToolResult,
    LlmToolSpec, LlmUsage, StopReason,
};

/// What the model was given (the text callback left aside).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<LlmToolSpec>,
}

impl RecordedRequest {
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name.clone()).collect()
    }

    pub fn last_message(&self) -> Option<&LlmMessage> {
        self.messages.last()
    }
}

/// Last tool results visible in a request (helper for scripted steps).
pub fn last_tool_results(request: &RecordedRequest) -> Vec<LlmToolResult> {
    match request.messages.last() {
        Some(LlmMessage::ToolResults { results }) => results.clone(),
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptedCall {
    pub name: String,
    pub input: Value,
    pub id: Option<String>,
}

pub fn call(name: &str, input: Value) -> ScriptedCall {
    ScriptedCall {
        name: name.into(),
        input,
        id: None,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptedReply {
    pub text: String,
    pub tool_calls: Vec<ScriptedCall>,
}

impl ScriptedReply {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tool_calls: Vec::new(),
        }
    }

    pub fn calls(text: impl Into<String>, tool_calls: Vec<ScriptedCall>) -> Self {
        Self {
            text: text.into(),
            tool_calls,
        }
    }
}

type Dynamic = Box<dyn Fn(&RecordedRequest) -> ScriptedReply + Send + Sync>;

pub enum ScriptedStep {
    Fixed(ScriptedReply),
    Dynamic(Dynamic),
}

impl ScriptedStep {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Fixed(ScriptedReply::text(text))
    }

    pub fn calls(text: impl Into<String>, tool_calls: Vec<ScriptedCall>) -> Self {
        Self::Fixed(ScriptedReply::calls(text, tool_calls))
    }

    pub fn dynamic(
        step: impl Fn(&RecordedRequest) -> ScriptedReply + Send + Sync + 'static,
    ) -> Self {
        Self::Dynamic(Box::new(step))
    }
}

pub struct ScriptedProvider {
    steps: Mutex<VecDeque<ScriptedStep>>,
    requests: Mutex<Vec<RecordedRequest>>,
}

impl ScriptedProvider {
    pub fn new(steps: Vec<ScriptedStep>) -> Self {
        Self {
            steps: Mutex::new(steps.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Queues more steps (tests driving a long-lived server).
    pub fn push(&self, steps: impl IntoIterator<Item = ScriptedStep>) {
        self.steps.lock().expect("steps").extend(steps);
    }

    pub fn remaining(&self) -> usize {
        self.steps.lock().expect("steps").len()
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("requests").clone()
    }
}

#[async_trait]
impl LlmProvider for ScriptedProvider {
    fn id(&self) -> &str {
        "scripted"
    }

    fn model(&self) -> &str {
        "scripted-model"
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let recorded = RecordedRequest {
            system: request.system.clone(),
            messages: request.messages.clone(),
            tools: request.tools.clone(),
        };
        let index = {
            let mut requests = self.requests.lock().expect("requests");
            requests.push(recorded.clone());
            requests.len()
        };
        let step = self.steps.lock().expect("steps").pop_front();
        let Some(step) = step else {
            panic!("ScriptedProvider: no step left (call #{index})");
        };
        let reply = match step {
            ScriptedStep::Fixed(reply) => reply,
            ScriptedStep::Dynamic(step) => step(&recorded),
        };
        let tool_calls: Vec<LlmToolCall> = reply
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(i, c)| LlmToolCall {
                id: c.id.unwrap_or_else(|| format!("call_{index}_{i}")),
                name: c.name,
                input: c.input,
            })
            .collect();
        if !reply.text.is_empty() {
            request.emit_text(&reply.text);
        }
        Ok(LlmResponse {
            text: reply.text,
            stop_reason: if tool_calls.is_empty() {
                StopReason::EndTurn
            } else {
                StopReason::ToolUse
            },
            tool_calls,
            usage: LlmUsage {
                input_tokens: 100,
                output_tokens: 20,
                ..LlmUsage::default()
            },
            provider: "scripted".into(),
            model: "scripted-model".into(),
            native: None,
        })
    }
}
