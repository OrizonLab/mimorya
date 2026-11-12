//! A company's standard mail as its administrators declare it (ADR-205):
//! its IMAP server, its SMTP server and, if it has one, its CalDAV agenda,
//! each where the network policy allows, no secret among them (each person
//! signs in with their own account); whether its tools may write; the names
//! of its tools, fixed by the platform as a company's policies name them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ImapServer, SmtpServer};
use crate::outbound::{NetworkPolicy, url_problem};

/// The connector type: one per company.
pub const MAIL_SERVER: &str = "mail_server";
/// Its tools, named by the platform: a company's policies may name them.
pub const MAIL_TOOLS: [&str; 5] = [
    "mail_search",
    "mail_read",
    "mail_calendar_events",
    "mail_send",
    "mail_calendar_event_create",
];
/// Its tools are named `mail_<tool>`.
pub const MAIL_NAMESPACE: &str = "mail_";

/// A company's mail servers, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailServerConfig {
    pub imap: ImapServer,
    pub smtp: SmtpServer,
    /// The address of its CalDAV agenda, over HTTPS, if it has one.
    #[serde(default)]
    pub caldav: Option<String>,
    /// Sending mail and creating events in the person's name, each
    /// validated.
    #[serde(default)]
    pub write_tools: bool,
}

impl MailServerConfig {
    /// The stored configuration, or why it cannot be read (in French).
    pub fn read(config: &Value) -> anyhow::Result<Self> {
        serde_json::from_value(config.clone()).map_err(|_| {
            anyhow::anyhow!(
                "Déclaration de la messagerie illisible : supprimez le connecteur puis ajoutez-le \
                 de nouveau"
            )
        })
    }

    /// Why one of its servers may not be used (in French), if one may not.
    pub fn problem(&self, policy: NetworkPolicy) -> Option<String> {
        self.imap
            .problem(policy)
            .or_else(|| self.smtp.problem(policy))
            .or_else(|| {
                let caldav = self.caldav.as_deref()?;
                url_problem(caldav, policy).map(|problem| format!("Agenda CalDAV : {problem}"))
            })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::standard_mail::SmtpSecurity;

    const SAAS: NetworkPolicy = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: false,
        proxy: None,
        test_servers: &[],
        test_roots: &[],
    };

    fn declared() -> MailServerConfig {
        MailServerConfig {
            imap: ImapServer {
                host: "imap.example.com".into(),
                port: 993,
            },
            smtp: SmtpServer {
                host: "smtp.example.com".into(),
                port: 587,
                security: SmtpSecurity::StartTls,
            },
            caldav: Some("https://dav.example.com/calendars/".into()),
            write_tools: false,
        }
    }

    #[test]
    fn reads_what_it_writes_and_nothing_else() {
        let stored = serde_json::to_value(declared()).unwrap();
        assert_eq!(
            stored,
            json!({
                "imap": { "host": "imap.example.com", "port": 993 },
                "smtp": { "host": "smtp.example.com", "port": 587, "security": "starttls" },
                "caldav": "https://dav.example.com/calendars/",
                "writeTools": false,
            })
        );
        assert_eq!(MailServerConfig::read(&stored).unwrap(), declared());
        let mut password = stored.clone();
        password["password"] = json!("secret");
        let refused = MailServerConfig::read(&password).unwrap_err().to_string();
        assert!(refused.contains("illisible"), "{refused}");
        // Without an agenda nor writing: as declared.
        let minimal = json!({
            "imap": { "host": "imap.example.com", "port": 993 },
            "smtp": { "host": "smtp.example.com", "port": 465, "security": "tls" },
        });
        let read = MailServerConfig::read(&minimal).unwrap();
        assert_eq!((read.caldav, read.write_tools), (None, false));
    }

    #[test]
    fn says_which_server_may_not_be_used() {
        assert_eq!(declared().problem(SAAS), None);
        let mut private = declared();
        private.smtp.host = "192.168.1.20".into();
        assert!(private.problem(SAAS).unwrap().starts_with("Serveur SMTP"));
        for caldav in [
            "http://dav.example.com/",
            "https://jean:motdepasse@dav.example.com/",
            "https://10.0.0.4/dav/",
        ] {
            let mut agenda = declared();
            agenda.caldav = Some(caldav.into());
            let problem = agenda.problem(SAAS).unwrap();
            assert!(problem.starts_with("Agenda CalDAV"), "{caldav}: {problem}");
        }
    }

    #[test]
    fn its_tools_are_in_its_namespace() {
        for tool in MAIL_TOOLS {
            assert!(tool.starts_with(MAIL_NAMESPACE), "{tool}");
        }
    }
}
