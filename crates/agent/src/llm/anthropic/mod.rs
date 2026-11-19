//! Anthropic adapter: Messages API, streamed, with prompt caching,
//! server-side fallback and the advisor.

mod advisor;
mod errors;
mod provider;
mod request;
mod sse;
mod stream;

pub use advisor::{ADVISOR_BETA, THINKING_BINDING_BETA};
pub use provider::{AnthropicOptions, AnthropicProvider, DEFAULT_BASE_URL};
pub use request::{
    DEFAULT_ANTHROPIC_MODEL, RequestSettings, SERVER_FALLBACK_BETA, betas, build_body,
    supports_server_fallback, to_anthropic_messages,
};
pub use sse::{SseEvent, SseParser};
pub use stream::{MessageAccumulator, map_stop_reason};
