//! The accounts a person linked at their company's providers, as a tool
//! reaches them (ADR-117): the access token of the run's person, for a kind
//! of connector, or why there is none; their account at the company's
//! standard mail, its password opened for one connection (ADR-205). A tool
//! never chooses whose account it reaches: the platform hands it the
//! account of the person the run serves.

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

/// Why a tool reaches no linked account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountAccessError {
    /// The person linked no account there (or removed it).
    NotLinked,
    /// The provider no longer honours the link (access revoked or expired):
    /// the link was removed, the person links their account again.
    Revoked,
    /// The person did not grant these permissions the tool needs (ADR-119):
    /// they link their account again to grant them.
    NotGranted(Vec<String>),
    /// Not reachable now: why, in French.
    Unavailable(String),
}

/// A run's way to the accounts its person linked.
#[async_trait]
pub trait LinkedAccounts: Send + Sync {
    /// A valid access token to the person's account at the company's
    /// connector of this type, once they granted the permissions in
    /// `needs`.
    async fn access_token(
        &self,
        connector_type: &str,
        needs: &[&str],
    ) -> Result<String, AccountAccessError>;

    /// The provider refused that token (revoked before it expired): the
    /// next call does not hand it again.
    async fn rejected(&self, _connector_type: &str) {}

    /// The run's person's account at their company's standard mail, with
    /// the servers it was checked against (ADR-205). None by default: only
    /// the platform's own port opens one.
    async fn mail_account(&self) -> Result<MailAccount, AccountAccessError> {
        Err(AccountAccessError::NotLinked)
    }

    /// The company's mail `server` refused that account (its password
    /// changed elsewhere): it is removed, unless linked again since, so that
    /// no run tries it again and gets the company's address banned.
    async fn mail_account_refused(&self, _linked_at: DateTime<Utc>, _server: MailServerRole) {}
}

/// Which of the company's mail servers a tool reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailServerRole {
    Imap,
    Smtp,
    CalDav,
}

impl MailServerRole {
    /// Its name in the audit log.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Imap => "imap",
            Self::Smtp => "smtp",
            Self::CalDav => "caldav",
        }
    }

    /// Its protocol, as a person reads it.
    pub fn protocol(self) -> &'static str {
        match self {
            Self::Imap => "IMAP",
            Self::Smtp => "SMTP",
            Self::CalDav => "CalDAV",
        }
    }
}

/// A person's account at their company's standard mail, opened for one
/// connection of a run that serves them: its password is never printed.
pub struct MailAccount {
    /// The company's declaration, as read with the account: the servers its
    /// password was checked against, never another.
    pub servers: Value,
    /// The address of the person's platform account: what they send from.
    pub address: String,
    pub username: String,
    password: String,
    /// When it was linked: what a refusal names, so that an account linked
    /// again since stays.
    pub linked_at: DateTime<Utc>,
}

impl MailAccount {
    pub fn new(
        servers: Value,
        address: String,
        username: String,
        password: String,
        linked_at: DateTime<Utc>,
    ) -> Self {
        Self {
            servers,
            address,
            username,
            password,
            linked_at,
        }
    }

    pub fn password(&self) -> &str {
        &self.password
    }
}

impl fmt::Debug for MailAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailAccount")
            .field("address", &self.address)
            .field("username", &self.username)
            .field("password", &"<masqué>")
            .field("linked_at", &self.linked_at)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_mail_account_never_prints_its_password() {
        let account = MailAccount::new(
            json!({}),
            "paul@exemple.fr".into(),
            "paul".into(),
            "secret-du-test".into(),
            Utc::now(),
        );
        let printed = format!("{account:?}");
        assert!(!printed.contains("secret-du-test"), "{printed}");
        assert_eq!(account.password(), "secret-du-test");
    }
}
