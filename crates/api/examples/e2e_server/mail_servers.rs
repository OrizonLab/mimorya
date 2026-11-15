//! The company's mail servers the end-to-end tests link Léa's account at
//! (ADR-205), at known addresses: an IMAP server on 4112, an SMTP server
//! asking for STARTTLS on 4113 and a CalDAV agenda on 4114, their
//! certificates of a test authority the connectors alone trust
//! (`connector_test_roots`), their addresses among the only private ones
//! they reach (`connector_test_servers`). `mail-account.spec.ts` declares
//! them and links her account there.

use std::net::SocketAddr;

use mimorya_connectors::outbound::TrustedRoot;
use mimorya_connectors::testing::{
    CalDavBehaviour, FakeCalDav, FakeImap, FakeSmtp, SmtpMode, SmtpSetup, TestAuthority,
};

const IMAP_ADDRESS: &str = "127.0.0.1:4112";
const SMTP_ADDRESS: &str = "127.0.0.1:4113";
const CALDAV_ADDRESS: &str = "127.0.0.1:4114";
/// Léa's account at them: her identifier and password.
const LEA_ACCOUNT: (&str, &str) = (
    "lea.fontaine@neo-maintenance.example",
    "Messagerie-Lea-2026!",
);

/// What the connectors reach them with.
pub struct MailServers {
    pub addresses: Vec<SocketAddr>,
    pub roots: Vec<TrustedRoot>,
}

/// Starts them, until the process ends.
pub async fn start() -> anyhow::Result<MailServers> {
    let authority = TestAuthority::new();
    let accounts = vec![(LEA_ACCOUNT.0.to_string(), LEA_ACCOUNT.1.to_string())];
    let imap = FakeImap::start_on(IMAP_ADDRESS.parse()?, &authority, accounts.clone()).await;
    let starttls = SmtpSetup::new(SmtpMode::StartTls);
    let smtp = FakeSmtp::with_on(
        SMTP_ADDRESS.parse()?,
        &authority,
        starttls,
        accounts.clone(),
    )
    .await;
    let behaviour = CalDavBehaviour::Answers;
    let agenda = FakeCalDav::start_on(
        CALDAV_ADDRESS.parse()?,
        &authority,
        accounts,
        behaviour,
        vec![],
    )
    .await;
    tracing::info!(agenda = agenda.base(), "End-to-end mail servers listening");
    Ok(MailServers {
        addresses: vec![imap.address, smtp.address, agenda.address],
        roots: authority.roots().to_vec(),
    })
}
