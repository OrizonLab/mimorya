//! What an inbound hook sends (ADR-109, ADR-110), as the end-to-end model
//! plays it: recognised by the envelope of the data it received, it is
//! answered apart, so that the reference scenarios queued for
//! conversations keep their order (`inbound-hooks.spec.ts`).

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmError, LlmProvider, LlmRequest, LlmResponse};

use crate::played;

/// How the data a hook received is enclosed in its turn.
const RECEIVED: &str = "<donnees_outil source=\"declencheur\"";
/// The answer `inbound-hooks.spec.ts` reads.
pub const ANSWER: &str = "Panne reçue de la supervision : une intervention est à planifier \
chez le client signalé.";

/// The platform model: what a hook sends is played here, the rest goes on.
pub struct WithHooks {
    others: Arc<dyn LlmProvider>,
}

impl WithHooks {
    pub fn new(others: Arc<dyn LlmProvider>) -> Self {
        Self { others }
    }
}

#[async_trait]
impl LlmProvider for WithHooks {
    fn id(&self) -> &str {
        self.others.id()
    }

    fn model(&self) -> &str {
        self.others.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let sent = request
            .messages
            .first()
            .is_some_and(|m| played::says(m, RECEIVED));
        if !sent {
            return self.others.complete(request).await;
        }
        Ok(played::reply(self, ANSWER, Vec::new()))
    }
}
