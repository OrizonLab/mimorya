//! The activities the Bot Framework sends a company's Teams bot that reach
//! the platform (ADR-138): a person's own message in their personal
//! conversation with the bot, in the company's own Microsoft Entra
//! directory, sent to the bot itself. Anything else (a group chat, a
//! channel, an installation, a reaction, another directory's person, an
//! activity for another bot) is taken and ignored. The service the activity
//! names is read first: its token must name it too (`token`).

use serde::Deserialize;
use uuid::Uuid;

use super::CHANNEL;

/// The longest message read: Teams takes about 28 kibibytes a message.
pub const MAX_MESSAGE_CHARS: usize = 4_000;
/// The longest identifier kept of an activity, a conversation or a Teams
/// user.
const MAX_ID_CHARS: usize = 512;
/// The longest activity identifier taken once (the events' column).
const MAX_ACTIVITY_ID_CHARS: usize = 64;
/// The longest name kept of a Teams user.
const MAX_NAME_CHARS: usize = 80;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawActivity {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    service_url: Option<String>,
    channel_id: Option<String>,
    from: Option<RawAccount>,
    recipient: Option<RawAccount>,
    conversation: Option<RawConversation>,
    channel_data: Option<RawChannelData>,
    text: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAccount {
    id: Option<String>,
    name: Option<String>,
    aad_object_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawConversation {
    id: Option<String>,
    conversation_type: Option<String>,
    tenant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawChannelData {
    tenant: Option<RawTenant>,
}

#[derive(Debug, Default, Deserialize)]
struct RawTenant {
    id: Option<String>,
}

/// An activity, read: the service it names, and what it holds.
#[derive(Debug)]
pub struct ReceivedActivity {
    pub service_url: String,
    raw: RawActivity,
}

/// A Teams user, as an activity names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamsUser {
    /// Their identifier for the bot (`29:…`).
    pub id: String,
    /// Their Microsoft Entra object identifier, lowercase.
    pub aad_object_id: String,
    pub name: Option<String>,
}

/// A person's own message to the bot, in their personal conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalMessage {
    pub activity_id: String,
    pub service_url: String,
    pub conversation_id: String,
    pub person: TeamsUser,
    pub text: String,
}

/// What the Bot Framework sent, if it reads as an activity naming its
/// service.
pub fn read_activity(body: &[u8]) -> Option<ReceivedActivity> {
    let raw: RawActivity = serde_json::from_slice(body).ok()?;
    let service_url = raw.service_url.clone().filter(|url| !url.is_empty())?;
    Some(ReceivedActivity { service_url, raw })
}

impl ReceivedActivity {
    /// The person's own message to the bot `app_id`, in their personal
    /// conversation, in the directory `directory_id`; `None` for anything
    /// else.
    pub fn personal_message(&self, app_id: &str, directory_id: &str) -> Option<PersonalMessage> {
        let raw = &self.raw;
        let conversation = raw.conversation.as_ref()?;
        let from = raw.from.as_ref()?;
        let to_the_bot =
            raw.recipient.as_ref().and_then(|r| r.id.as_deref()) == Some(&format!("28:{app_id}"));
        let tenant = (raw.channel_data.as_ref())
            .and_then(|data| data.tenant.as_ref())
            .and_then(|tenant| tenant.id.as_deref())
            .or(conversation.tenant_id.as_deref());
        let personal = raw.kind.as_deref() == Some("message")
            && raw.channel_id.as_deref() == Some(CHANNEL)
            && conversation.conversation_type.as_deref() == Some("personal")
            && to_the_bot
            && tenant.is_some_and(|tenant| same_guid(tenant, directory_id));
        if !personal {
            return None;
        }
        let activity_id = (raw.id.as_deref()).filter(|id| bounded_id(id, MAX_ACTIVITY_ID_CHARS))?;
        let conversation_id =
            (conversation.id.as_deref()).filter(|id| bounded_id(id, MAX_ID_CHARS))?;
        let person_id = (from.id.as_deref())
            .filter(|id| id.starts_with("29:") && bounded_id(id, MAX_ID_CHARS))?;
        let aad_object_id = from.aad_object_id.as_deref().and_then(guid)?;
        let text = raw.text.as_deref().unwrap_or_default().trim();
        if text.chars().count() > MAX_MESSAGE_CHARS {
            return None;
        }
        let name = (from.name.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.chars().any(char::is_control))
            .map(|name| name.chars().take(MAX_NAME_CHARS).collect());
        Some(PersonalMessage {
            activity_id: activity_id.to_owned(),
            service_url: self.service_url.clone(),
            conversation_id: conversation_id.to_owned(),
            person: TeamsUser {
                id: person_id.to_owned(),
                aad_object_id,
                name,
            },
            text: text.to_owned(),
        })
    }
}

/// A Microsoft Entra identifier (a directory, an application, an object),
/// lowercase, if `value` is one.
pub fn guid(value: &str) -> Option<String> {
    let parsed = Uuid::parse_str(value.trim()).ok()?;
    // As Microsoft writes them, hyphenated, nothing else.
    (parsed.hyphenated().to_string() == value.trim().to_ascii_lowercase())
        .then(|| parsed.hyphenated().to_string())
}

/// Whether two identifiers are the same Entra identifier.
pub fn same_guid(left: &str, right: &str) -> bool {
    matches!((guid(left), guid(right)), (Some(left), Some(right)) if left == right)
}

/// An identifier as the Bot Framework writes them: printable, bounded.
fn bounded_id(id: &str, max: usize) -> bool {
    !id.is_empty() && id.chars().count() <= max && id.chars().all(|c| c.is_ascii_graphic())
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
