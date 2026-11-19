//! Provider-neutral LLM contract. Adapters translate to and from each
//! provider's wire format; the runtime only sees these types.

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmToolCall {
    pub id: String,
    pub name: String,
    /// Parsed arguments, still unvalidated: the policy enforcement point validates them.
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmToolResult {
    pub tool_call_id: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

/// Assistant turns keep the provider's native content so they can be replayed
/// verbatim on the next request (required by providers that bind reasoning
/// blocks to the conversation, and good for prompt caching).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeContent {
    pub provider: String,
    pub content: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "role",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum LlmMessage {
    User {
        content: String,
    },
    Assistant {
        content: String,
        tool_calls: Vec<LlmToolCall>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        native: Option<NativeContent>,
    },
    ToolResults {
        results: Vec<LlmToolResult>,
    },
}

impl LlmMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant {
            content: content.into(),
            tool_calls: Vec::new(),
            native: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

/// Receives streamed text as it is generated.
pub type TextSink = Arc<dyn Fn(&str) + Send + Sync>;

/// Anthropic's advisor (ADR-207, step 3): a stronger model the model may
/// consult while it answers, a server tool of that provider alone; no other
/// is ever given one. Kept unchanged for a whole run: changing it would
/// edit the tools its reasoning is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorTool {
    pub model: String,
    /// The consultations a turn may have.
    pub max_uses: u32,
    /// Whether the advisor's own prompt is cached between consultations.
    pub cache: bool,
}

/// A consultation of the advisor during a call: its model and its tokens,
/// billed at that model's prices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consultation {
    pub model: String,
    pub usage: LlmUsage,
}

#[derive(Clone)]
pub struct LlmRequest {
    /// Stable instructions. Keep volatile data out of it to preserve prompt caching.
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<LlmToolSpec>,
    pub max_output_tokens: Option<u32>,
    pub effort: Option<Effort>,
    pub on_text_delta: Option<TextSink>,
    /// The advisor the model may consult (Anthropic only).
    pub advisor: Option<AdvisorTool>,
    /// The tools changed since reasoning in the history was produced (the
    /// advisor withdrawn): that reasoning is dropped rather than refused.
    pub drop_unbound_thinking: bool,
}

impl fmt::Debug for LlmRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlmRequest")
            .field("messages", &self.messages.len())
            .field("tools", &self.tools.len())
            .field("effort", &self.effort)
            .field("advisor", &self.advisor)
            .finish_non_exhaustive()
    }
}

impl LlmRequest {
    pub fn new(system: impl Into<String>, messages: Vec<LlmMessage>) -> Self {
        Self {
            system: system.into(),
            messages,
            tools: Vec::new(),
            max_output_tokens: None,
            effort: None,
            on_text_delta: None,
            advisor: None,
            drop_unbound_thinking: false,
        }
    }

    pub fn emit_text(&self, text: &str) {
        if let Some(sink) = &self.on_text_delta {
            sink(text);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    /// The provider paused the turn, a server tool (the advisor) still
    /// running: the same request goes on from the paused answer.
    PauseTurn,
    Other,
}

/// Token counts of one model call. `input_tokens` excludes the tokens read
/// from or written to the cache: each provider adapter splits them so.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl LlmUsage {
    /// Every input token, cached or not.
    pub fn input_total(&self) -> u64 {
        self.input_tokens + self.cache_read_tokens + self.cache_write_tokens
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Vec<LlmToolCall>,
    pub stop_reason: StopReason,
    pub usage: LlmUsage,
    pub provider: String,
    /// Model that actually served the request (may differ after a server-side fallback).
    pub model: String,
    pub native: Option<NativeContent>,
    /// The advisor's consultations during the call, apart from its usage.
    pub consultations: Vec<Consultation>,
    /// Whether a consultation of the advisor is still to run, at the start
    /// of the next request: the turn paused, or the model called it beside
    /// tools. That request must give the advisor again.
    pub advisor_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmErrorKind {
    Auth,
    RateLimit,
    Overloaded,
    InvalidRequest,
    ContextOverflow,
    Network,
    MalformedOutput,
    Aborted,
    Unknown,
}

/// A provider failure. Its message is safe to show to users (French).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct LlmError {
    pub provider: String,
    pub kind: LlmErrorKind,
    pub message: String,
}

impl LlmError {
    pub fn new(provider: &str, kind: LlmErrorKind, message: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            kind,
            message: message.into(),
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self.kind,
            LlmErrorKind::RateLimit
                | LlmErrorKind::Overloaded
                | LlmErrorKind::Network
                | LlmErrorKind::MalformedOutput
        )
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Identifier used in usage records, e.g. "anthropic", "openrouter".
    fn id(&self) -> &str;
    fn model(&self) -> &str;
    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError>;
}
