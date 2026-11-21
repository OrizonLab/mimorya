//! Provider-neutral LLM layer: types, adapters (Anthropic, OpenAI-compatible),
//! routing per task class, price estimation, and the embedding models that
//! search memory by meaning.

pub mod advisor;
pub mod anthropic;
pub mod embedding;
pub mod escalation;
pub mod factory;
pub mod openai_compatible;
pub mod pricing;
pub mod router;
pub mod settings;
pub mod types;

pub use advisor::{AdvisorOffer, advisor_pair_accepted, advisor_tool, advisors_for};
pub use embedding::{
    EMBEDDING_DIMENSIONS, Embedded, EmbeddingConfig, EmbeddingProvider, TenantEmbeddingChoice,
};
pub use escalation::{
    AVAILABLE_MECHANISMS, ESCALATION_BUDGET_ROLES, EscalationChoice, EscalationRequest,
    EscalationRoute, EscalationRoutes, MAX_ESCALATION_BUDGET_MICROS, MAX_ESCALATIONS_PER_RUN,
    MIN_ESCALATION_BUDGET_MICROS, Mechanism, RunKind, check_escalation, escalation_problem,
};
pub use factory::{PROVIDER_KINDS, ProviderConfig, create_provider, default_endpoint};
pub use pricing::{
    JEV_PRICES, JEV_PRICES_READ_ON, JEV_PRICES_SOURCE, LongPromptPrice, ModelPrice, PriceTable,
    TierPrice,
};
pub use router::{ModelRole, ModelRouter, TaskClass};
pub use settings::{
    TenantLlmChoice, api_key_problem, check_tenant_llm, company_provider_config, default_model,
    official_endpoint, requires_api_key, uses_listed_endpoint,
};
pub use types::{
    AdvisorTool, Consultation, Effort, LlmError, LlmErrorKind, LlmMessage, LlmProvider, LlmRequest,
    LlmResponse, LlmToolCall, LlmToolResult, LlmToolSpec, LlmUsage, NativeContent, StopReason,
    TextSink,
};
