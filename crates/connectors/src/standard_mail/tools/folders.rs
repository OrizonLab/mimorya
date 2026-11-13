//! Where the mail tools look (ADR-205): the person's inbox, or the folder
//! their server keeps the e-mails they sent in (marked `\Sent`, RFC 6154,
//! else named as the common hosts name it). A folder is always examined,
//! read only: nothing read is marked as read. An e-mail is named by its
//! folder, the folder's `UIDVALIDITY` and its UID, so that a number the
//! server gave again to another e-mail is never taken for it.

use std::sync::LazyLock;

use async_imap::types::NameAttribute;
use futures::StreamExt;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::person::{PersonSession, failure};
use crate::outbound::Cause;

/// What a server keeping no folder of sent e-mails is told.
pub const NO_SENT_FOLDER: &str =
    "Le serveur n’indique pas de dossier des e-mails envoyés pour ce compte";
/// What an e-mail no longer where it was named is told.
pub const MOVED: &str = "Cet e-mail n’est plus à cette place : cherchez-le de nouveau";
/// The most a folder list may hold.
const LIST_BYTES: usize = 256 * 1024;
/// The names the common hosts give the folder of sent e-mails, for a server
/// that marks none (as it lists them, modified UTF-7 for « Éléments
/// envoyés »).
const SENT_NAMES: [&str; 6] = [
    "Sent",
    "Sent Items",
    "Sent Messages",
    "INBOX.Sent",
    "Envoy&AOk-s",
    "&AMk-l&AOk-ments envoy&AOk-s",
];

/// An e-mail's name: `i` (inbox) or `s` (sent), the folder's validity, the
/// e-mail's UID.
pub const MESSAGE_ID_PATTERN: &str = r"^[is][0-9]{1,10}\.[0-9]{1,10}$";
pub static MESSAGE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(MESSAGE_ID_PATTERN).expect("valid regex"));

/// A folder the tools look in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum Folder {
    /// La boîte de réception
    #[default]
    #[serde(rename = "inbox")]
    Inbox,
    /// Les e-mails envoyés
    #[serde(rename = "sent")]
    Sent,
}

impl Folder {
    fn letter(self) -> char {
        match self {
            Folder::Inbox => 'i',
            Folder::Sent => 's',
        }
    }
}

/// A folder examined: its validity and how many e-mails it holds.
#[derive(Debug, Clone, Copy)]
pub struct Examined {
    pub folder: Folder,
    pub validity: u32,
    pub exists: u32,
}

impl Examined {
    /// The name of its e-mail `uid`.
    pub fn id(&self, uid: u32) -> String {
        format!("{}{}.{uid}", self.folder.letter(), self.validity)
    }
}

/// An e-mail's name, read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageId {
    pub folder: Folder,
    pub validity: u32,
    pub uid: u32,
}

impl MessageId {
    pub fn parse(id: &str) -> Option<Self> {
        let folder = match id.chars().next()? {
            'i' => Folder::Inbox,
            's' => Folder::Sent,
            _ => return None,
        };
        let (validity, uid) = id.get(1..)?.split_once('.')?;
        Some(Self {
            folder,
            validity: validity.parse().ok()?,
            uid: uid.parse().ok().filter(|uid| *uid > 0)?,
        })
    }
}

/// `folder`, examined.
pub async fn examine(person: &mut PersonSession, folder: Folder) -> Result<Examined, Cause> {
    let name = match folder {
        Folder::Inbox => "INBOX".to_string(),
        Folder::Sent => sent_folder(person).await?,
    };
    let mailbox = match person.session.examine(&name).await {
        Ok(mailbox) => mailbox,
        Err(error) => return Err(person.failed(error)),
    };
    Ok(Examined {
        folder,
        validity: mailbox.uid_validity.unwrap_or_default(),
        exists: mailbox.exists,
    })
}

/// The folder the server keeps sent e-mails in.
async fn sent_folder(person: &mut PersonSession) -> Result<String, Cause> {
    person.grant(LIST_BYTES);
    let budget = person.budget.clone();
    let listed = person
        .session
        .list(Some(""), Some("*"))
        .await
        .map_err(|error| failure(&budget, error))?;
    let mut listed = Box::pin(listed);
    let mut names = Vec::new();
    while let Some(name) = listed.next().await {
        let name = name.map_err(|error| failure(&budget, error))?;
        let sent = name
            .attributes()
            .iter()
            .any(|attribute| matches!(attribute, NameAttribute::Sent));
        names.push((sent, name.name().to_string()));
    }
    names
        .iter()
        .find(|(sent, _)| *sent)
        .or_else(|| {
            SENT_NAMES
                .iter()
                .find_map(|common| names.iter().find(|(_, name)| name == common))
        })
        .map(|(_, name)| name.clone())
        .ok_or(Cause {
            summary: NO_SENT_FOLDER,
            detail: String::new(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_e_mail_is_named_by_its_folder_validity_and_uid() {
        let examined = Examined {
            folder: Folder::Sent,
            validity: 1_700_000_000,
            exists: 3,
        };
        let id = examined.id(42);
        assert_eq!(id, "s1700000000.42");
        assert!(MESSAGE_ID_RE.is_match(&id));
        assert_eq!(
            MessageId::parse(&id),
            Some(MessageId {
                folder: Folder::Sent,
                validity: 1_700_000_000,
                uid: 42
            })
        );
        for wrong in ["x1.2", "i1", "i1.0", "i.2", "i1.2.3", "i99999999999.1"] {
            assert_eq!(MessageId::parse(wrong), None, "{wrong}");
        }
    }
}
