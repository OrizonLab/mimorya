//! The accounts a person linked at their company's providers, as a tool
//! reaches them (ADR-117): the access token of the run's person, for a kind
//! of connector, or why there is none. A tool never chooses whose account
//! it reaches: the platform hands it the token of the person the run serves.

use async_trait::async_trait;

/// Why a tool reaches no linked account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountAccessError {
    /// The person linked no account there (or removed it).
    NotLinked,
    /// The provider no longer honours the link (access revoked or expired):
    /// the link was removed, the person links their account again.
    Revoked,
    /// Not reachable now: why, in French.
    Unavailable(String),
}

/// A run's way to the accounts its person linked.
#[async_trait]
pub trait LinkedAccounts: Send + Sync {
    /// A valid access token to the person's account at the company's
    /// connector of this type.
    async fn access_token(&self, connector_type: &str) -> Result<String, AccountAccessError>;

    /// The provider refused that token (revoked before it expired): the
    /// next call does not hand it again.
    async fn rejected(&self, _connector_type: &str) {}
}

/// Where no linked account is reached: a run that may not (an
/// evaluation), or a test. Its reason, in French, is what a tool says.
pub struct NoLinkedAccounts(pub &'static str);

#[async_trait]
impl LinkedAccounts for NoLinkedAccounts {
    async fn access_token(&self, _connector_type: &str) -> Result<String, AccountAccessError> {
        Err(AccountAccessError::Unavailable(self.0.into()))
    }
}
