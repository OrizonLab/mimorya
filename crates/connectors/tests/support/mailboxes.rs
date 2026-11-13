//! What the tests of the standard mail's tools share (ADR-205): the folders
//! of a simulated IMAP server, Paul's account linked there with the
//! declaration it goes with, and the tools behind the policy enforcement
//! point, started trusting the test authority.

use std::net::SocketAddr;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::ToolExecutor;
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::standard_mail::MailServerConnector;
use mimorya_connectors::testing::{
    FakeImap, ImapBehaviour, ImapSetup, Mailboxes, TestAuthority, TestFolder, TestMessage,
};
use mimorya_connectors::types::{Connector, InstanceContext};
use mimorya_core::demo::user;
use mimorya_core::tools::{LinkedAccounts, ToolRef};
use serde_json::{Value, json};
use uuid::Uuid;

use super::linked_accounts::mailbox_executor;
use super::mail::{PASSWORD, USER, accounts, trusting};

pub const VALIDITY: u32 = 1_700_000_000;

pub fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
}

/// An e-mail of `from`, about `subject`, saying `text`.
pub fn message(uid: u32, seen: bool, d: u32, subject: &str, text: &str) -> TestMessage {
    let raw = format!(
        "From: Fournisseur <contact@fournisseur.example>\r\nTo: {USER}\r\n\
         Subject: {subject}\r\nDate: Tue, {d} Nov 2026 09:30:00 +0100\r\n\r\n{text}\r\n"
    );
    TestMessage {
        uid,
        seen,
        day: day(d),
        raw,
    }
}

/// An e-mail written in HTML with an attachment, its subject encoded.
pub fn with_attachment(uid: u32) -> TestMessage {
    let raw = "From: =?UTF-8?Q?Ren=C3=A9e_Dupont?= <renee@exemple.fr>\r\n\
To: Paul <paul@neo-maintenance.example>\r\nCc: lea@exemple.fr\r\n\
Subject: =?ISO-8859-1?Q?R=E9union_de_chantier?=\r\n\
Date: Mon, 9 Nov 2026 08:00:00 +0100\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"b1\"\r\n\r\n--b1\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<p>Bonjour,<br>la <b>réunion</b> de chantier est lundi.</p>\r\n--b1\r\n\
Content-Type: application/pdf\r\n\
Content-Disposition: attachment; filename=\"devis.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--b1--\r\n";
    TestMessage {
        uid,
        seen: false,
        day: day(9),
        raw: raw.into(),
    }
}

/// An e-mail enclosing an e-mail thirty thousand deep, in less than a
/// megabyte, as anyone may send one.
pub fn enclosing_without_end(uid: u32) -> TestMessage {
    let mut raw = format!("From: renee@exemple.fr\r\nTo: {USER}\r\nSubject: Emboîtés\r\n");
    raw.push_str(&"Content-Type: message/rfc822\r\n\r\n".repeat(30_000));
    raw.push_str("Fin\r\n");
    TestMessage {
        uid,
        seen: false,
        day: day(11),
        raw,
    }
}

/// An inbox of three e-mails (UIDs 3, 8, 12) and, if `sent` names it, a
/// folder of sent e-mails.
pub fn folders(sent: Option<(&str, bool)>) -> Mailboxes {
    let inbox = TestFolder {
        name: "INBOX".into(),
        sent: false,
        validity: VALIDITY,
        messages: vec![
            message(3, true, 2, "Facture de mars", "Montant : 120 €"),
            with_attachment(8),
            message(
                12,
                false,
                10,
                "Relance facture",
                "Merci de régler la facture",
            ),
        ],
    };
    let mut folders = vec![inbox];
    if let Some((name, marked)) = sent {
        folders.push(TestFolder {
            name: name.into(),
            sent: marked,
            validity: 42,
            messages: vec![message(5, true, 6, "Devis envoyé", "Voici le devis")],
        });
    }
    Mailboxes {
        folders,
        literal_plus: true,
        ..Mailboxes::default()
    }
}

/// The company's declaration naming `imap`.
pub fn declaration(imap: SocketAddr) -> Value {
    json!({
        "imap": { "host": imap.ip().to_string(), "port": imap.port() },
        "smtp": { "host": imap.ip().to_string(), "port": 465, "security": "tls" },
    })
}

pub fn linked_at() -> DateTime<Utc> {
    DateTime::from_timestamp(1_790_000_000, 0).unwrap()
}

/// A simulated server holding `mailboxes`, and Paul's ports, his account
/// linked there with `password`.
pub async fn server(
    authority: &TestAuthority,
    mailboxes: Mailboxes,
    password: &str,
) -> (FakeImap, Arc<InMemoryPorts>) {
    let setup = ImapSetup::new(ImapBehaviour::Answers);
    let imap = FakeImap::holding(authority, accounts(), setup, mailboxes).await;
    let ports = InMemoryPorts::new(user::PAUL, Uuid::new_v4());
    ports.link_mail_account(declaration(imap.address), USER, password, linked_at());
    (imap, ports)
}

/// The tools started on `declared`, under `network`.
pub async fn tools(declared: &Value, network: NetworkPolicy) -> Vec<ToolRef> {
    let context = InstanceContext {
        network,
        ..InstanceContext::default()
    };
    MailServerConnector
        .instantiate(declared, context)
        .await
        .unwrap()
        .tools
}

/// Paul's tools behind the policy enforcement point, the company's
/// declaration naming his server.
pub async fn paul(
    authority: &TestAuthority,
    imap: &FakeImap,
    ports: &Arc<InMemoryPorts>,
) -> ToolExecutor {
    let tools = tools(&declaration(imap.address), trusting(authority)).await;
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    mailbox_executor(tools, user::PAUL, ports, accounts)
}

/// Paul, his account linked with the password the server accepts.
pub async fn linked_paul(
    authority: &TestAuthority,
    mailboxes: Mailboxes,
) -> (FakeImap, Arc<InMemoryPorts>, ToolExecutor) {
    let (imap, ports) = server(authority, mailboxes, PASSWORD).await;
    let executor = paul(authority, &imap, &ports).await;
    (imap, ports, executor)
}
