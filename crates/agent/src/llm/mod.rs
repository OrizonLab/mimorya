//! Provider-neutral LLM layer: types, adapters (Anthropic, OpenAI-compatible),
//! routing per task class and price estimation.

pub mod anthropic;
pub mod factory;
pub mod openai_compatible;
pub mod pricing;
pub mod router;
pub mod settings;
pub mod types;

pub use factory::{PROVIDER_KINDS, ProviderConfig, create_provider};
pub use pricing::{ModelPrice, PriceTable};
pub use router::{ModelRouter, TaskClass};
pub use settings::{
    TenantLlmChoice, api_key_problem, check_tenant_llm, company_provider_config, default_model,
    official_endpoint, requires_api_key, uses_listed_endpoint,
};
pub use types::{
    Effort, LlmError, LlmErrorKind, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall,
    LlmToolResult, LlmToolSpec, LlmUsage, NativeContent, StopReason, TextSink,
};
