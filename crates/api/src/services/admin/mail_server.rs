//! The company's standard mail, declared by its connector administrators
//! (ADR-205): one per company, under the administration lock, each of its
//! servers where the network policy allows, the declaration audited. It
//! holds no secret: each person signs in with their own account. Its
//! servers change one by one (`mail_changed`, for `connector_config`).

use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::standard_mail::{ImapServer, MailServerConfig, SmtpServer};
use mimorya_core::DomainError;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::connector_config::ConfigChanges;
use super::connectors::{ConnectorAdmin, PERMISSION};
use super::link_applications::{Declaration, Declared, declare_once};
use crate::services::AccessContext;

pub struct NewMailServer {
    pub name: String,
    pub config: MailServerConfig,
    pub enabled: bool,
}

/// A change of its servers: `None` keeps each.
#[derive(Default)]
pub struct MailChanges {
    pub imap: Option<ImapServer>,
    pub smtp: Option<SmtpServer>,
    /// `Some(None)` removes its agenda.
    pub caldav: Option<Option<String>>,
}

impl MailChanges {
    /// What the audit keeps of it: each server it changes, before and
    /// after.
    pub fn audited(&self, stored: &Value) -> Option<Value> {
        let mut changed = Map::new();
        let mut note = |field: &str, after: Value| {
            let before = stored.get(field).cloned().unwrap_or(Value::Null);
            if before != after {
                changed.insert(field.into(), json!({ "before": before, "after": after }));
            }
        };
        if let Some(imap) = &self.imap {
            note("imap", json!(imap));
        }
        if let Some(smtp) = &self.smtp {
            note("smtp", json!(smtp));
        }
        if let Some(caldav) = &self.caldav {
            note("caldav", json!(caldav));
        }
        (!changed.is_empty()).then_some(Value::Object(changed))
    }
}

pub async fn create_mail_server_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    input: NewMailServer,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    if let Some(problem) = input.config.problem(admin.connectors.network()) {
        return Err(DomainError::validation(problem).into());
    }
    let config = serde_json::to_value(&input.config)?;
    let declaration = Declaration {
        declared: Declared::MailServer,
        name: input.name,
        enabled: input.enabled,
        config,
        secret: None,
        audited: Map::from_iter([
            ("imap".into(), json!(input.config.imap)),
            ("smtp".into(), json!(input.config.smtp)),
            ("caldav".into(), json!(input.config.caldav)),
            ("writeTools".into(), json!(input.config.write_tools)),
        ]),
    };
    declare_once(admin, access, declaration).await
}

/// Whether a change moves the mail to another host, one a person's
/// password would then go to without their having signed in there: the
/// accounts linked at it are let go.
pub(super) fn hosts_moved(before: &Value, after: &Value) -> bool {
    match (
        MailServerConfig::read(before),
        MailServerConfig::read(after),
    ) {
        (Ok(before), Ok(after)) => before.hosts() != after.hosts(),
        _ => false,
    }
}

/// The configuration once these changes apply, its servers checked again.
pub(super) fn mail_changed(
    mut config: MailServerConfig,
    changes: ConfigChanges,
    network: NetworkPolicy,
) -> Result<Value, DomainError> {
    if changes.url.is_some() {
        return Err(DomainError::validation(
            "La messagerie standard n’a pas d’adresse unique : changez son serveur IMAP, son \
             serveur SMTP ou son agenda CalDAV",
        ));
    }
    if changes.rest.is_some() {
        return Err(DomainError::validation(
            "Seule une API REST a un en-tête de jeton et des opérations",
        ));
    }
    let mail = changes.mail.unwrap_or_default();
    if let Some(imap) = mail.imap {
        config.imap = imap;
    }
    if let Some(smtp) = mail.smtp {
        config.smtp = smtp;
    }
    if let Some(caldav) = mail.caldav {
        config.caldav = caldav;
    }
    if let Some(write_tools) = changes.write_tools {
        config.write_tools = write_tools;
    }
    if let Some(problem) = config.problem(network) {
        return Err(DomainError::validation(problem));
    }
    serde_json::to_value(&config).map_err(|_| DomainError::validation("Déclaration illisible"))
}

#[cfg(test)]
mod tests {
    use mimorya_connectors::standard_mail::SmtpSecurity;

    use super::*;

    fn declared() -> MailServerConfig {
        MailServerConfig {
            imap: ImapServer {
                host: "imap.example.com".into(),
                port: 993,
            },
            smtp: SmtpServer {
                host: "smtp.example.com".into(),
                port: 465,
                security: SmtpSecurity::Tls,
            },
            caldav: None,
            write_tools: false,
        }
    }

    #[test]
    fn the_audit_keeps_each_server_changed_before_and_after() {
        let stored = serde_json::to_value(declared()).unwrap();
        let changes = MailChanges {
            imap: Some(declared().imap),
            smtp: Some(SmtpServer {
                port: 587,
                security: SmtpSecurity::StartTls,
                ..declared().smtp
            }),
            caldav: Some(Some("https://dav.example.com/".into())),
        };
        assert_eq!(
            changes.audited(&stored).unwrap(),
            json!({
                "smtp": {
                    "before": { "host": "smtp.example.com", "port": 465, "security": "tls" },
                    "after": { "host": "smtp.example.com", "port": 587, "security": "starttls" },
                },
                "caldav": { "before": null, "after": "https://dav.example.com/" },
            })
        );
        let same = MailChanges {
            imap: Some(declared().imap),
            ..MailChanges::default()
        };
        assert_eq!(same.audited(&stored), None);
    }

    #[test]
    fn only_another_host_lets_the_accounts_go() {
        let before = serde_json::to_value(declared()).unwrap();
        let mut ported = declared();
        ported.smtp.port = 587;
        ported.smtp.security = SmtpSecurity::StartTls;
        ported.write_tools = true;
        let ported = serde_json::to_value(ported).unwrap();
        assert!(!hosts_moved(&before, &ported));
        let mut moved = declared();
        moved.imap.host = "mail.example.org".into();
        assert!(hosts_moved(&before, &serde_json::to_value(moved).unwrap()));
        let mut agenda = declared();
        agenda.caldav = Some("https://dav.example.com/".into());
        assert!(hosts_moved(&before, &serde_json::to_value(agenda).unwrap()));
    }

    #[test]
    fn its_servers_change_one_by_one_each_checked() {
        let network = NetworkPolicy::default();
        let moved = ConfigChanges {
            mail: Some(MailChanges {
                caldav: Some(Some("https://dav.example.com/".into())),
                ..MailChanges::default()
            }),
            write_tools: Some(true),
            ..ConfigChanges::default()
        };
        let stored = mail_changed(declared(), moved, network).unwrap();
        assert_eq!(stored["caldav"], "https://dav.example.com/");
        assert_eq!(stored["imap"]["host"], "imap.example.com");
        assert_eq!(stored["writeTools"], true);
        let private = ConfigChanges {
            mail: Some(MailChanges {
                imap: Some(ImapServer {
                    host: "127.0.0.1".into(),
                    port: 993,
                }),
                ..MailChanges::default()
            }),
            ..ConfigChanges::default()
        };
        let refused = mail_changed(declared(), private, network).unwrap_err();
        assert!(refused.message.starts_with("Serveur IMAP"), "{refused:?}");
        let url = ConfigChanges {
            url: Some("https://mail.example.com/".into()),
            ..ConfigChanges::default()
        };
        let refused = mail_changed(declared(), url, network).unwrap_err();
        assert!(refused.message.contains("pas d’adresse unique"));
    }
}
