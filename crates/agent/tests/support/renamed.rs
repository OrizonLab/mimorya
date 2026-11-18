//! A model whose responses name another model than the one it was asked
//! for, as a provider may answer with a dated snapshot: its responses name
//! the scripted model, which has no price, and it is asked as
//! `claude-opus-4-8`, which has one.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmError, LlmProvider, LlmRequest, LlmResponse};
use mimorya_agent::testing::ScriptedProvider;

pub struct Renamed(pub Arc<ScriptedProvider>);

#[async_trait]
impl LlmProvider for Renamed {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        "claude-opus-4-8"
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        self.0.complete(request).await
    }
}
