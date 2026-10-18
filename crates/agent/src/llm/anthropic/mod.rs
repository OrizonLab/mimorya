//! Anthropic adapter: Messages API, streamed, with prompt caching and
//! server-side fallback.

mod errors;
mod provider;
mod request;
mod sse;
mod stream;

pub use provider::{AnthropicOptions, AnthropicProvider, DEFAULT_BASE_URL};
pub use request::{
    DEFAULT_ANTHROPIC_MODEL, RequestSettings, SERVER_FALLBACK_BETA, build_body,
    supports_server_fallback, to_anthropic_messages,
};
pub use sse::{SseEvent, SseParser};
pub use stream::{MessageAccumulator, map_stop_reason};
