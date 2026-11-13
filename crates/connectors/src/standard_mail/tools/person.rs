//! The run's person at their company's mail servers (ADR-205): their
//! account taken from the platform with the declaration it was read with
//! (the servers its password was checked against, never another), checked
//! again against the network policy (`account`); their IMAP session, signed
//! in over TLS, what the server sends bounded, left once the tool's work is
//! done (`open`). A refused account is reported to the platform, which
//! removes it: a password changed elsewhere is not tried again by each run.
//! Every failure is said in French, the password out of it.

use std::borrow::Cow;
use std::time::Duration;

use async_imap::error::Error as ImapError;
use chrono::{DateTime, Utc};
use mimorya_core::tools::{AccountAccessError, MailAccount, MailServerRole, ToolContext};
use tokio::time::timeout;

use crate::outbound::{Cause, NetworkPolicy, ReadBudget, TOO_LONG, TOO_SLOW, UNREACHABLE};
use crate::standard_mail::literals::io_failure;
use crate::standard_mail::{
    CREDENTIALS_REFUSED, ImapSession, LEAVE_TIMEOUT, MailLogin, MailServerConfig,
    NOT_A_MAIL_SERVER, imap_session,
};

/// What a server refusing a command is told.
pub const COMMAND_REFUSED: &str = "Le serveur IMAP refuse la demande";
/// The most of what a server says kept in what the model is told.
const MAX_SAID_CHARS: usize = 200;

/// How the mail tools reach the company's IMAP server.
#[derive(Debug, Clone, Copy)]
pub struct MailReach {
    pub network: NetworkPolicy,
    /// How long one call may take, from the account's opening to the
    /// session's end.
    pub call_timeout: Duration,
}

/// A session of the run's person, and what it may still read.
pub struct PersonSession {
    pub session: ImapSession,
    pub budget: ReadBudget,
    login: MailLogin,
}

impl MailReach {
    /// `work` done within the call's delay, or said too slow.
    pub async fn within<T>(
        &self,
        work: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        timeout(self.call_timeout, work)
            .await
            .unwrap_or_else(|_| Err(told_plain(TOO_SLOW, "")))
    }

    /// The run's person's account, the servers it was read with, and how
    /// it signs in; or why not, in French.
    pub async fn account(&self, ctx: &ToolContext) -> Result<PersonAccount, String> {
        let account = ctx.accounts.mail_account().await.map_err(not_reached)?;
        let config = MailServerConfig::read(&account.servers).map_err(|e| e.to_string())?;
        if let Some(problem) = config.problem(self.network) {
            return Err(problem);
        }
        let login = MailLogin::new(&account.username, account.password())?;
        Ok(PersonAccount {
            account,
            config,
            login,
        })
    }

    /// A session of the run's person, or why there is none, in French.
    pub async fn open(&self, ctx: &ToolContext) -> Result<PersonSession, String> {
        let PersonAccount {
            account,
            config,
            login,
        } = self.account(ctx).await?;
        match imap_session(&config.imap, &login, self.network).await {
            Ok((session, budget)) => Ok(PersonSession {
                session,
                budget,
                login,
            }),
            Err(cause) if cause.summary == CREDENTIALS_REFUSED => {
                Err(refused(ctx, MailServerRole::Imap, account.linked_at).await)
            }
            Err(cause) => Err(told(&login, cause)),
        }
    }
}

/// The run's person's account at their company's mail.
pub struct PersonAccount {
    pub account: MailAccount,
    /// The servers it was read with.
    pub config: MailServerConfig,
    pub login: MailLogin,
}

/// The company's `server` refused the account linked at `linked_at`: the
/// platform removes it, and the model is told.
pub async fn refused(
    ctx: &ToolContext,
    server: MailServerRole,
    linked_at: DateTime<Utc>,
) -> String {
    ctx.accounts.mail_account_refused(linked_at, server).await;
    format!(
        "Le serveur {} de l’entreprise refuse le compte relié (mot de passe changé ?) : il est \
         retiré, la personne le relie de nouveau dans « Mon compte »",
        server.protocol()
    )
}

impl PersonSession {
    /// Leaves the session, the work done: what it found, or why not, as the
    /// model reads it.
    pub async fn finish<T>(self, worked: Result<T, Cause>) -> Result<T, String> {
        let Self {
            mut session, login, ..
        } = self;
        // The answer known: a logout the server answers badly changes
        // nothing.
        let _ = timeout(LEAVE_TIMEOUT, session.logout()).await;
        worked.map_err(|cause| told(&login, cause))
    }

    /// `bytes` more may be read for the next step.
    pub fn grant(&self, bytes: usize) {
        self.budget.grant(bytes);
    }

    /// What failed in a step, told by what the server or the connection
    /// did.
    pub fn failed(&self, error: ImapError) -> Cause {
        failure(&self.budget, error)
    }
}

/// What failed in a step on a session reading on `budget`.
pub fn failure(budget: &ReadBudget, error: ImapError) -> Cause {
    let (summary, detail): (&'static str, Cow<'_, str>) = match &error {
        ImapError::No(said) | ImapError::Bad(said) => (COMMAND_REFUSED, said.into()),
        ImapError::Io(error) => {
            let (summary, detail) = io_failure(error);
            (summary, detail.into())
        }
        ImapError::ConnectionLost => (UNREACHABLE, "connexion perdue".into()),
        other => (NOT_A_MAIL_SERVER, other.to_string().into()),
    };
    let summary = if budget.spent() { TOO_LONG } else { summary };
    Cause {
        summary,
        detail: detail.into_owned(),
    }
}

/// Why the run's person's account is not reached, as the model reads it.
fn not_reached(error: AccountAccessError) -> String {
    match error {
        AccountAccessError::NotLinked => "La personne servie n’a relié aucun compte à la \
                                          messagerie de l’entreprise : elle le relie dans « Mon \
                                          compte »"
            .into(),
        AccountAccessError::Revoked | AccountAccessError::NotGranted(_) => {
            "Le compte de messagerie relié n’est plus utilisable : la personne le relie de \
             nouveau dans « Mon compte »"
                .into()
        }
        AccountAccessError::Unavailable(why) => why,
    }
}

/// A failure as the model reads it: its summary, then what the server said,
/// the password out of it, abridged.
pub fn told(login: &MailLogin, cause: Cause) -> String {
    told_plain(cause.summary, &login.scrub(&cause.detail))
}

pub fn told_plain(summary: &str, detail: &str) -> String {
    let detail: String = detail
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_SAID_CHARS)
        .collect();
    match detail.trim() {
        "" => summary.to_string(),
        said => format!("{summary} ({said})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_never_carries_the_password_nor_a_line_break() {
        let login = MailLogin::new("paul", "secret\"à\\part").unwrap();
        let cause = Cause {
            summary: COMMAND_REFUSED,
            detail: "NO \"secret\\\"à\\\\part\" refusé\r\nA2 LOGOUT".into(),
        };
        let said = told(&login, cause);
        assert!(!said.contains("secret"), "{said}");
        assert!(!said.contains('\n') && !said.contains('\r'), "{said}");
        assert!(said.starts_with(COMMAND_REFUSED), "{said}");
        assert_eq!(told_plain(TOO_SLOW, " "), TOO_SLOW);
    }
}
