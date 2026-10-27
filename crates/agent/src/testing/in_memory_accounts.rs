//! The accounts the in-memory ports' person linked (ADR-117): a token per
//! kind of connector, set by the test, with the permissions it grants
//! (ADR-119). For tests only.

use async_trait::async_trait;
use mimorya_core::account_links::missing_scopes;
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};

use super::in_memory_ports::InMemoryPorts;

/// A link: its token, and the permissions granted, if the test named them.
#[derive(Clone)]
pub(crate) struct Link {
    token: String,
    granted: Option<String>,
}

impl InMemoryPorts {
    /// The person linked an account at the company's connector of this
    /// type, granting every permission; its tools reach it with this token.
    pub fn link_account(&self, connector_type: &str, token: &str) {
        self.link(connector_type, token, None);
    }

    /// The same, granting only these permissions (as a provider lists
    /// them): a tool needing another is refused (ADR-119).
    pub fn link_account_granting(&self, connector_type: &str, token: &str, granted: &str) {
        self.link(connector_type, token, Some(granted.into()));
    }

    fn link(&self, connector_type: &str, token: &str, granted: Option<String>) {
        let link = Link {
            token: token.into(),
            granted,
        };
        self.state()
            .linked_accounts
            .insert(connector_type.into(), link);
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
        needs: &[&str],
    ) -> Result<String, AccountAccessError> {
        let link = self
            .state()
            .linked_accounts
            .get(connector_type)
            .cloned()
            .ok_or(AccountAccessError::NotLinked)?;
        let missing = link
            .granted
            .as_deref()
            .map_or_else(Vec::new, |granted| missing_scopes(granted, needs));
        if !missing.is_empty() {
            return Err(AccountAccessError::NotGranted(
                missing.into_iter().map(String::from).collect(),
            ));
        }
        Ok(link.token)
    }

    async fn rejected(&self, connector_type: &str) {
        self.state().rejected_accounts.push(connector_type.into());
    }
}
