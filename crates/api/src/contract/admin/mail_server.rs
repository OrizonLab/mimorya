//! A company's standard mail as the administration declares and reads it
//! (`connector:manage`, ADR-205): its IMAP and SMTP servers and its CalDAV
//! agenda, if any, and how many people linked their account there. It
//! holds no secret: each person signs in with their own account.

use mimorya_connectors::standard_mail::{ImapServer, MailServerConfig, SmtpSecurity, SmtpServer};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::{trimmed, trimmed_opt};

/// The IMAP server, over implicit TLS (993 usually).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailImapServer {
    #[serde(deserialize_with = "trimmed")]
    pub host: String,
    pub port: u16,
}

/// How the SMTP server is secured: TLS from the first byte (465 usually),
/// or STARTTLS, which it must offer (587 usually).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MailSmtpSecurity {
    Tls,
    StartTls,
}

/// The SMTP server a person's e-mails leave through.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailSmtpServer {
    #[serde(deserialize_with = "trimmed")]
    pub host: String,
    pub port: u16,
    pub security: MailSmtpSecurity,
}

/// Body of `POST /admin/connectors/mail-server`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewMailServerConnector {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    pub imap: MailImapServer,
    pub smtp: MailSmtpServer,
    /// The address of its CalDAV agenda (`https://…`). Absent or blank:
    /// none.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub caldav: Option<String>,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
    /// Whether its tools may send e-mail and create events in the person's
    /// name, each validated. Absent: they only read.
    #[serde(default)]
    #[ts(optional)]
    pub write_tools: Option<bool>,
}

/// A company's standard mail, as `ConnectorView` shows it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MailServerView {
    pub imap: MailImapServer,
    pub smtp: MailSmtpServer,
    pub caldav: Option<String>,
    /// Its tools may send e-mail and create events.
    pub write_tools: bool,
    /// How many people linked their account there.
    pub accounts: i64,
}

impl From<MailImapServer> for ImapServer {
    fn from(server: MailImapServer) -> Self {
        Self {
            host: server.host,
            port: server.port,
        }
    }
}

impl From<ImapServer> for MailImapServer {
    fn from(server: ImapServer) -> Self {
        Self {
            host: server.host,
            port: server.port,
        }
    }
}

impl From<MailSmtpServer> for SmtpServer {
    fn from(server: MailSmtpServer) -> Self {
        Self {
            host: server.host,
            port: server.port,
            security: match server.security {
                MailSmtpSecurity::Tls => SmtpSecurity::Tls,
                MailSmtpSecurity::StartTls => SmtpSecurity::StartTls,
            },
        }
    }
}

impl From<SmtpServer> for MailSmtpServer {
    fn from(server: SmtpServer) -> Self {
        Self {
            host: server.host,
            port: server.port,
            security: match server.security {
                SmtpSecurity::Tls => MailSmtpSecurity::Tls,
                SmtpSecurity::StartTls => MailSmtpSecurity::StartTls,
            },
        }
    }
}

impl MailServerView {
    pub fn new(config: MailServerConfig, accounts: i64) -> Self {
        Self {
            imap: config.imap.into(),
            smtp: config.smtp.into(),
            caldav: config.caldav,
            write_tools: config.write_tools,
            accounts,
        }
    }
}
