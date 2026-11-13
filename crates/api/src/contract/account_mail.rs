//! The account the signed-in person links at their company's standard
//! mail (ADR-205), as their account page reads and writes it: the hosts
//! their password would go to, and the identifier they linked; never the
//! password.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::{iso, trimmed};
use crate::services::mail_accounts::MailAccountSeen;

/// `GET /account/mail`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountMail {
    /// The company's standard mail, if the person may link an account
    /// there or linked one.
    pub mail: Option<MailAccountView>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MailAccountView {
    pub connector_id: Uuid,
    pub name: String,
    /// An account may be linked: the mail is enabled and the person may
    /// use connectors.
    pub available: bool,
    /// The hosts the person's password goes to: its IMAP and SMTP servers'
    /// and its agenda's.
    pub hosts: Vec<String>,
    pub linked: Option<LinkedMailAccount>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LinkedMailAccount {
    pub username: String,
    pub linked_at: String,
}

impl From<MailAccountSeen> for MailAccountView {
    fn from(seen: MailAccountSeen) -> Self {
        Self {
            connector_id: seen.connector_id,
            name: seen.name,
            available: seen.available,
            hosts: seen.hosts,
            linked: seen.linked.map(|(username, linked_at)| LinkedMailAccount {
                username,
                linked_at: iso(&linked_at),
            }),
        }
    }
}

/// Body of `PUT /account/mail`: the identifier and password the person
/// signs in with at their company's mail servers. The password is taken
/// as typed, spaces included, and never printed.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailAccountLink {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 320))]
    pub username: String,
    #[validate(length(min = 1, max = 1024))]
    pub password: String,
}
