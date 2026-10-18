//! Chooses a provider per task class. Tenants can route verification to a
//! different (cheaper, or local) model; by default every class uses the agent
//! provider.

use std::sync::Arc;

use super::types::LlmProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskClass {
    Agent,
    Verification,
}

#[derive(Clone)]
pub struct ModelRouter {
    agent: Arc<dyn LlmProvider>,
    verification: Option<Arc<dyn LlmProvider>>,
}

impl ModelRouter {
    pub fn new(agent: Arc<dyn LlmProvider>) -> Self {
        Self {
            agent,
            verification: None,
        }
    }

    pub fn with_verification(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.verification = Some(provider);
        self
    }

    pub fn provider_for(&self, task: TaskClass) -> Arc<dyn LlmProvider> {
        match (task, &self.verification) {
            (TaskClass::Verification, Some(provider)) => provider.clone(),
            _ => self.agent.clone(),
        }
    }
}
