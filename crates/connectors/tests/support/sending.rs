//! What the tests of the standard mail's `mail_send` share (ADR-205): a
//! company's declaration naming a simulated SMTP server, its tools allowed
//! to write; Paul, his account linked there; the e-mail he validates, and
//! the text of one the server took.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::ToolExecutor;
use mimorya_connectors::standard_mail::MailServerConnector;
use mimorya_connectors::testing::{FakeSmtp, SmtpMode, SmtpSetup, TestAuthority};
use mimorya_connectors::types::{Connector, InstanceContext};
use mimorya_core::demo::user;
use mimorya_core::tools::{LinkedAccounts, ToolRef};
use serde_json::{Value, json};
use uuid::Uuid;

use super::linked_accounts::mailbox_executor;
use super::mail::{PASSWORD, USER, accounts, trusting};
use super::mailboxes::linked_at;

/// The company's declaration naming `smtp`, its tools allowed to write.
pub fn declaration(smtp: &FakeSmtp, security: &str) -> Value {
    let host = smtp.address.ip().to_string();
    json!({
        "imap": { "host": host, "port": 993 },
        "smtp": { "host": host, "port": smtp.address.port(), "security": security },
        "writeTools": true,
    })
}

/// The tools started on `declared`, each call within `call_timeout`.
pub async fn tools(
    authority: &TestAuthority,
    declared: &Value,
    call_timeout: Duration,
) -> Vec<ToolRef> {
    let context = InstanceContext {
        network: trusting(authority),
        call_timeout,
        ..InstanceContext::default()
    };
    MailServerConnector
        .instantiate(declared, context)
        .await
        .unwrap()
        .tools
}

pub struct Sending {
    pub smtp: FakeSmtp,
    pub ports: Arc<InMemoryPorts>,
    pub paul: ToolExecutor,
}

/// Paul, his account linked as `username` with `password` at a simulated
/// SMTP server set up so, each call within `call_timeout`.
pub async fn sending_as(
    setup: SmtpSetup,
    (username, password): (&str, &str),
    call_timeout: Duration,
) -> Sending {
    let authority = TestAuthority::new();
    let smtp = FakeSmtp::with(&authority, setup, accounts()).await;
    let security = match setup.mode {
        SmtpMode::StartTls => "starttls",
        _ => "tls",
    };
    let declared = declaration(&smtp, security);
    let ports = InMemoryPorts::new(user::PAUL, Uuid::new_v4());
    ports.link_mail_account(declared.clone(), username, password, linked_at());
    let tools = tools(&authority, &declared, call_timeout).await;
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let paul = mailbox_executor(tools, user::PAUL, &ports, accounts);
    Sending { smtp, ports, paul }
}

/// The same, with the account the server accepts.
pub async fn sending_paul_with(setup: SmtpSetup, call_timeout: Duration) -> Sending {
    sending_as(setup, (USER, PASSWORD), call_timeout).await
}

pub async fn sending_paul(setup: SmtpSetup) -> Sending {
    sending_paul_with(setup, Duration::from_secs(20)).await
}

/// The e-mail Paul validates: two recipients, one again in copy.
pub fn email() -> Value {
    json!({
        "to": ["claire@exemple.fr", "marc@exemple.fr"],
        "cc": ["CLAIRE@exemple.fr"],
        "subject": "Planning de novembre — révisé",
        "body": "Bonjour,\n.\nLe planning est prêt.\n\tPaul",
    })
}

/// The decoded text of a delivered e-mail.
pub fn decoded_body(text: &str) -> String {
    let (_, body) = text.split_once("\r\n\r\n").unwrap();
    let joined: String = body.split("\r\n").collect();
    String::from_utf8(STANDARD.decode(joined).unwrap()).unwrap()
}
