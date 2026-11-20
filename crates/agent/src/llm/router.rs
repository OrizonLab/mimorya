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

/// The role a model call plays, recorded with what it cost (ADR-207): the
/// main model, the escalation model, a classification of the run, the
/// advisor it consulted, or the review of an answer (ADR-108).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    Main,
    Escalation,
    Classification,
    Advisor,
    Review,
    /// A question of the decision service (ADR-213).
    Decision,
}

impl ModelRole {
    pub const ALL: [ModelRole; 6] = [
        ModelRole::Main,
        ModelRole::Escalation,
        ModelRole::Classification,
        ModelRole::Advisor,
        ModelRole::Review,
        ModelRole::Decision,
    ];

    /// As recorded (`usage_records_role_check`).
    pub fn as_str(self) -> &'static str {
        match self {
            ModelRole::Main => "main",
            ModelRole::Escalation => "escalation",
            ModelRole::Classification => "classification",
            ModelRole::Advisor => "advisor",
            ModelRole::Review => "review",
            ModelRole::Decision => "decision",
        }
    }
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

    /// The model the company chose apart to review answers, if any
    /// (ADR-108): the run's own reviews should it fail, each within the
    /// operator's supply (`runtime/final_answer.rs`, ADR-208).
    pub fn reviewer(&self) -> Option<Arc<dyn LlmProvider>> {
        self.verification.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::ScriptedProvider;

    #[test]
    fn the_review_model_is_the_one_routed_apart_if_any() {
        let agent: Arc<dyn LlmProvider> = Arc::new(ScriptedProvider::new(Vec::new()));
        let judge: Arc<dyn LlmProvider> = Arc::new(ScriptedProvider::new(Vec::new()));
        let alone = ModelRouter::new(agent.clone());
        assert!(alone.reviewer().is_none());
        let routed = alone.with_verification(judge.clone());
        assert!(Arc::ptr_eq(&routed.reviewer().unwrap(), &judge));
        assert!(Arc::ptr_eq(&routed.provider_for(TaskClass::Agent), &agent));
        assert!(Arc::ptr_eq(
            &routed.provider_for(TaskClass::Verification),
            &judge
        ));
    }
}
