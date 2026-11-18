//! Provider-neutral LLM layer: types, adapters (Anthropic, OpenAI-compatible),
//! routing per task class, price estimation, and the embedding models that
//! search memory by meaning.

pub mod anthropic;
pub mod embedding;
pub mod escalation;
pub mod factory;
pub mod openai_compatible;
pub mod pricing;
pub mod router;
pub mod settings;
pub mod types;

pub use embedding::{
    EMBEDDING_DIMENSIONS, Embedded, EmbeddingConfig, EmbeddingProvider, TenantEmbeddingChoice,
};
pub use escalation::{
    AVAILABLE_MECHANISMS, EscalationChoice, EscalationRequest, MAX_ESCALATION_BUDGET_MICROS,
    MAX_ESCALATIONS_PER_RUN, MIN_ESCALATION_BUDGET_MICROS, Mechanism, check_escalation,
    escalation_problem,
};
pub use factory::{PROVIDER_KINDS, ProviderConfig, create_provider};
pub use pricing::{LongPromptPrice, ModelPrice, PriceTable, TierPrice};
pub use router::{ModelRole, ModelRouter, TaskClass};
pub use settings::{
    TenantLlmChoice, api_key_problem, check_tenant_llm, company_provider_config, default_model,
    official_endpoint, requires_api_key, uses_listed_endpoint,
};
pub use types::{
    Effort, LlmError, LlmErrorKind, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall,
    LlmToolResult, LlmToolSpec, LlmUsage, NativeContent, StopReason, TextSink,
};
