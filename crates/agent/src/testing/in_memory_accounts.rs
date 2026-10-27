//! The accounts the in-memory ports' person linked (ADR-117): a token per
//! kind of connector, set by the test. For tests only.

use async_trait::async_trait;
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};

use super::in_memory_ports::InMemoryPorts;

impl InMemoryPorts {
    /// The person linked an account at the company's connector of this
    /// type, granting every permission; its tools reach it with this token.
    pub fn link_account(&self, connector_type: &str, token: &str) {
        self.state()
            .linked_accounts
            .insert(connector_type.into(), token.into());
    }

    /// The kinds of connector whose token a tool reported refused.
    pub fn rejected_accounts(&self) -> Vec<String> {
        self.state().rejected_accounts.clone()
    }
}

#[async_trait]
impl LinkedAccounts for InMemoryPorts {
    async fn access_token(
        &self,
        connector_type: &str,
        _needs: &[&str],
    ) -> Result<String, AccountAccessError> {
        self.state()
            .linked_accounts
            .get(connector_type)
            .cloned()
            .ok_or(AccountAccessError::NotLinked)
    }

    async fn rejected(&self, connector_type: &str) {
        self.state().rejected_accounts.push(connector_type.into());
    }
}
