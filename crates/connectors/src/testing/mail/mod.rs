//! A company's mail servers as the tests simulate them (ADR-205): an IMAP
//! server over implicit TLS (`imap`), with the folders a test gives
//! (`mailbox`), an SMTP server over implicit TLS, over STARTTLS, or in clear
//! without it (`smtp`), with the e-mails it took (`delivery`) and what it
//! may do instead of answering a command (`instead`), each with a
//! certificate of a test authority and its accounts, and each noting what it
//! heard, never a password.

mod delivery;
mod imap;
mod instead;
mod lines;
mod mailbox;
mod smtp;

pub use delivery::{AfterData, Deliveries, Delivery};
pub use imap::{FakeImap, ImapBehaviour, ImapSetup};
pub use instead::Instead;
pub use mailbox::{FetchBehaviour, Mailboxes, TestFolder, TestMessage};
pub use smtp::{FakeSmtp, SmtpMode, SmtpSetup};

use std::sync::{Arc, Mutex};

/// The accounts a simulated server accepts: identifier and password.
pub type Accounts = Vec<(String, String)>;

/// What a simulated server heard, in order, credentials masked.
#[derive(Default, Clone)]
pub struct Heard(Arc<Mutex<Vec<String>>>);

impl Heard {
    pub fn lines(&self) -> Vec<String> {
        self.0.lock().expect("heard").clone()
    }

    fn note(&self, line: impl Into<String>) {
        self.0.lock().expect("heard").push(line.into());
    }
}

fn accepts(accounts: &Accounts, username: &str, password: &str) -> bool {
    accounts
        .iter()
        .any(|(known, secret)| known == username && secret == password)
}
