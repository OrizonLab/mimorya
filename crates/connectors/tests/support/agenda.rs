//! What the tests of the standard mail's agenda share (ADR-205): the
//! company's declaration naming a simulated CalDAV agenda, the events it
//! holds, Paul with his account linked there, and the week read.

use std::sync::Arc;
use std::time::Duration;

use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::ToolExecutor;
use mimorya_connectors::testing::{CalDavBehaviour, FakeCalDav, TestAuthority};
use mimorya_core::demo::user;
use mimorya_core::tools::LinkedAccounts;
use serde_json::{Value, json};
use uuid::Uuid;

use super::linked_accounts::mailbox_executor;
use super::mail::{USER, accounts};
use super::mailboxes::linked_at;
use super::sending::tools;

/// The company's declaration naming `agenda`.
pub fn declaration(agenda: &FakeCalDav) -> Value {
    json!({
        "imap": { "host": "127.0.0.1", "port": 993 },
        "smtp": { "host": "127.0.0.1", "port": 465, "security": "tls" },
        "caldav": agenda.base(),
    })
}

pub fn events() -> Vec<String> {
    [
        "BEGIN:VEVENT\r\nUID:visite@neo\r\nSUMMARY:Visite client\r\n\
         DTSTART;TZID=Europe/Paris:20261117T140000\r\nDTEND;TZID=Europe/Paris:20261117T150000\r\n\
         END:VEVENT\r\n",
        "BEGIN:VEVENT\r\nUID:chantier@neo\r\nSUMMARY:Réunion de chantier\r\n\
         DTSTART:20261116T080000Z\r\nDTEND:20261116T090000Z\r\nLOCATION:Quai B\r\n\
         DESCRIPTION:Ordre du jour\\, sécurité\r\nORGANIZER;CN=Léa:mailto:lea@neo.example\r\n\
         ATTENDEE;CN=Paul:mailto:paul@neo.example\r\nEND:VEVENT\r\n",
        "BEGIN:VEVENT\r\nUID:conges@neo\r\nSUMMARY:Congés\r\nDTSTART;VALUE=DATE:20261118\r\n\
         DTEND;VALUE=DATE:20261119\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\n",
        "BEGIN:VEVENT\r\nUID:plus-tard@neo\r\nSUMMARY:Hors période\r\n\
         DTSTART:20261220T080000Z\r\nDTEND:20261220T090000Z\r\nEND:VEVENT\r\n",
    ]
    .map(String::from)
    .to_vec()
}

/// Paul, his account linked with `password`, at an agenda behaving so.
pub async fn paul_at(
    behaviour: CalDavBehaviour,
    password: &str,
) -> (FakeCalDav, Arc<InMemoryPorts>, ToolExecutor) {
    paul_holding(behaviour, password, events()).await
}

/// The same, the agenda holding `held`.
pub async fn paul_holding(
    behaviour: CalDavBehaviour,
    password: &str,
    held: Vec<String>,
) -> (FakeCalDav, Arc<InMemoryPorts>, ToolExecutor) {
    let authority = TestAuthority::new();
    let agenda = FakeCalDav::start(&authority, accounts(), behaviour, held).await;
    let declared = declaration(&agenda);
    let ports = InMemoryPorts::new(user::PAUL, Uuid::new_v4());
    ports.link_mail_account(declared.clone(), USER, password, linked_at());
    let tools = tools(&authority, &declared, Duration::from_secs(20)).await;
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let paul = mailbox_executor(tools, user::PAUL, &ports, accounts);
    (agenda, ports, paul)
}

pub fn week() -> Value {
    json!({ "from": "2026-11-16", "to": "2026-11-18" })
}
