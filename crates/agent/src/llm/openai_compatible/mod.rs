//! OpenAI-compatible Chat Completions adapter (OpenAI, OpenRouter, Gemini,
//! Mistral, Ollama, vLLM…).

pub(crate) mod errors;
mod messages;
mod provider;

pub use messages::{map_finish_reason, parse_arguments, to_openai_messages};
pub use provider::{OPENAI_BASE_URL, OpenAiCompatibleOptions, OpenAiCompatibleProvider};
