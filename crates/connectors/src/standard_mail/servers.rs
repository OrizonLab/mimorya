//! Where a company's mail servers are (ADR-205): its IMAP server, always over
//! implicit TLS, and its SMTP server, over implicit TLS or STARTTLS, each a
//! host and a port the network policy allows.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::outbound::{NetworkPolicy, host_problem};

/// The IMAP server: implicit TLS on its port (993 usually).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImapServer {
    pub host: String,
    pub port: u16,
}

/// How the SMTP server is secured before anything is said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    /// TLS from the first byte (465 usually).
    Tls,
    /// STARTTLS, which the server must offer (587 usually).
    StartTls,
}

/// The SMTP server a person's e-mails leave through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SmtpServer {
    pub host: String,
    pub port: u16,
    pub security: SmtpSecurity,
}

impl ImapServer {
    /// Why it may not be used (in French), if it may not.
    pub fn problem(&self, policy: NetworkPolicy) -> Option<String> {
        host_problem(&self.host, self.port, policy)
            .map(|problem| format!("Serveur IMAP : {problem}"))
    }
}

impl SmtpServer {
    /// Why it may not be used (in French), if it may not.
    pub fn problem(&self, policy: NetworkPolicy) -> Option<String> {
        host_problem(&self.host, self.port, policy)
            .map(|problem| format!("Serveur SMTP : {problem}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAAS: NetworkPolicy = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: false,
        proxy: None,
        test_servers: &[],
        test_roots: &[],
    };

    #[test]
    fn reads_and_writes_as_the_declaration_holds_them() {
        let smtp: SmtpServer = serde_json::from_value(serde_json::json!({
            "host": "smtp.example.com", "port": 587, "security": "starttls"
        }))
        .unwrap();
        assert_eq!(smtp.security, SmtpSecurity::StartTls);
        assert_eq!(
            serde_json::to_value(SmtpSecurity::Tls).unwrap(),
            serde_json::json!("tls")
        );
        let unknown = serde_json::json!({ "host": "imap.example.com", "port": 993, "tls": false });
        assert!(serde_json::from_value::<ImapServer>(unknown).is_err());
    }

    #[test]
    fn says_which_server_may_not_be_used() {
        let imap = ImapServer {
            host: "10.0.0.8".into(),
            port: 993,
        };
        assert!(imap.problem(SAAS).unwrap().starts_with("Serveur IMAP"));
        let smtp = SmtpServer {
            host: "smtp..example.com".into(),
            port: 465,
            security: SmtpSecurity::Tls,
        };
        assert!(smtp.problem(SAAS).unwrap().starts_with("Serveur SMTP"));
        let fine = ImapServer {
            host: "imap.example.com".into(),
            port: 993,
        };
        assert_eq!(fine.problem(SAAS), None);
    }
}
