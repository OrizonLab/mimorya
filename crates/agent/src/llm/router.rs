//! Chooses a provider per task class. Tenants can route verification to a
//! different (cheaper, or local) model (ADR-108); by default every class uses
//! the agent provider.

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

    /// The providers to try for `task`, in order: for verification, the one
    /// routed apart, then the agent's should it fail (ADR-108).
    pub fn providers_for(&self, task: TaskClass) -> Vec<Arc<dyn LlmProvider>> {
        match (task, &self.verification) {
            (TaskClass::Verification, Some(provider)) => vec![provider.clone(), self.agent.clone()],
            _ => vec![self.agent.clone()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::ScriptedProvider;

    fn ids(providers: &[Arc<dyn LlmProvider>], agent: &Arc<dyn LlmProvider>) -> Vec<bool> {
        providers.iter().map(|p| Arc::ptr_eq(p, agent)).collect()
    }

    #[test]
    fn verification_falls_back_to_the_agent_provider() {
        let agent: Arc<dyn LlmProvider> = Arc::new(ScriptedProvider::new(Vec::new()));
        let judge: Arc<dyn LlmProvider> = Arc::new(ScriptedProvider::new(Vec::new()));
        let alone = ModelRouter::new(agent.clone());
        assert_eq!(
            ids(&alone.providers_for(TaskClass::Verification), &agent),
            [true]
        );
        let routed = alone.with_verification(judge);
        assert_eq!(
            ids(&routed.providers_for(TaskClass::Verification), &agent),
            [false, true]
        );
        assert_eq!(ids(&routed.providers_for(TaskClass::Agent), &agent), [true]);
    }
}
