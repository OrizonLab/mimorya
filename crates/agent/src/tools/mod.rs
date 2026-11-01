//! Tools as seen by the engine: the registry of a tenant, the policy
//! enforcement point, the untrusted-data envelope and the platform tools.

pub mod executor;
pub mod platform;
pub mod registry;
pub mod untrusted;

pub use executor::{ToolCallEvaluation, ToolExecutor};
pub use registry::{RegisteredTool, ToolOverride, ToolRegistry, to_tool_spec};
pub use untrusted::{
    neutralise_envelopes, neutralise_tags, tags_named, wrap_tool_result, wrap_untrusted,
};
