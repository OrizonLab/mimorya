//! The accounts the in-memory ports' person linked (ADR-117): a token per
//! kind of connector, set by the test, with the permissions it grants
//! (ADR-119); and their account at the company's standard mail, with the
//! servers it was checked against (ADR-205). For tests only.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mimorya_core::account_links::LinkKind;
use mimorya_core::tools::{AccountAccessError, LinkedAccounts, MailAccount};
use serde_json::Value;

use super::in_memory_ports::InMemoryPorts;

/// A link: its token, and the permissions granted, if the test named them.
#[derive(Clone)]
pub(crate) struct Link {
    token: String,
    granted: Option<String>,
}

/// An account at the standard mail: the declaration it goes with, the
/// account, when it was linked.
#[derive(Clone)]
pub(crate) struct MailLink {
    servers: Value,
    username: String,
    password: String,
    linked_at: DateTime<Utc>,
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

    /// The person linked this account at the company's standard mail,
    /// declared as `servers`, on `linked_at`.
    pub fn link_mail_account(
        &self,
        servers: Value,
        username: &str,
        password: &str,
        linked_at: DateTime<Utc>,
    ) {
        self.state().mail_account = Some(MailLink {
            servers,
            username: username.into(),
            password: password.into(),
            linked_at,
        });
    }

    /// When the mail accounts a tool reported refused were linked.
    pub fn refused_mail_accounts(&self) -> Vec<DateTime<Utc>> {
        self.state().refused_mail_accounts.clone()
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
        let missing = match (link.granted.as_deref(), LinkKind::of(connector_type)) {
            (Some(granted), Some(kind)) => kind.missing_scopes(granted, needs),
            _ => Vec::new(),
        };
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

    async fn mail_account(&self) -> Result<MailAccount, AccountAccessError> {
        let link = self
            .state()
            .mail_account
            .clone()
            .ok_or(AccountAccessError::NotLinked)?;
        Ok(MailAccount::new(
            link.servers,
            link.username,
            link.password,
            link.linked_at,
        ))
    }

    async fn mail_account_refused(&self, linked_at: DateTime<Utc>) {
        let mut state = self.state();
        state.refused_mail_accounts.push(linked_at);
        if state
            .mail_account
            .as_ref()
            .is_some_and(|link| link.linked_at == linked_at)
        {
            state.mail_account = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn a_refusal_never_removes_a_mail_account_linked_again_since() {
        let ports = InMemoryPorts::new(Uuid::new_v4(), Uuid::new_v4());
        let first = Utc::now() - Duration::minutes(5);
        let again = Utc::now();
        ports.link_mail_account(json!({}), "paul", "ancien", first);
        ports.link_mail_account(json!({}), "paul", "nouveau", again);
        ports.mail_account_refused(first).await;
        let kept = ports.mail_account().await.unwrap();
        assert_eq!(kept.password(), "nouveau");
        ports.mail_account_refused(again).await;
        assert_eq!(
            ports.mail_account().await.err(),
            Some(AccountAccessError::NotLinked)
        );
        assert_eq!(ports.refused_mail_accounts(), [first, again]);
    }
}
