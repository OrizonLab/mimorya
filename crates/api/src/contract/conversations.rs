//! Conversations of the signed-in user, their messages and the runs they
//! started; starting, renaming or archiving one, and sending a message.

use mimorya_core::tools::Citation as ToolCitation;
use mimorya_db::repositories::conversations::{ConversationRow, MessageRow};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::runs::{RunRef, RunStatus, Verification};
use super::stored::{stored, stored_text};
use crate::http::{iso, trimmed, trimmed_opt};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: Uuid,
    pub title: String,
    pub updated_at: String,
    /// The service it is held in a thread of, `slack`: its author's
    /// messages there follow it, and are answered there (ADR-127).
    pub gateway: Option<String>,
}

impl ConversationSummary {
    /// `c`, held in a thread of `gateway`, if any.
    pub fn held(c: &ConversationRow, gateway: Option<String>) -> Self {
        Self {
            id: c.id,
            title: c.title.clone(),
            updated_at: iso(&c.updated_at),
            gateway,
        }
    }
}

/// A conversation opened in the application: held in no thread.
impl From<&ConversationRow> for ConversationSummary {
    fn from(c: &ConversationRow) -> Self {
        Self::held(c, None)
    }
}

/// `GET /conversations`.
#[derive(Debug, Serialize, TS)]
pub struct ConversationList {
    pub conversations: Vec<ConversationSummary>,
}

/// `POST /conversations`.
#[derive(Debug, Serialize, TS)]
pub struct ConversationCreated {
    pub conversation: ConversationSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

/// A source the agent's answer relies on: an identifier returned by a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
}

impl From<&ToolCitation> for Citation {
    fn from(c: &ToolCitation) -> Self {
        Self {
            kind: c.kind.clone(),
            id: c.id.clone(),
            label: c.label.clone(),
        }
    }
}

/// What the platform records with a message (`messages.metadata`): written
/// with this type (`services::runs`), read back with it, a key off the
/// contract being an error.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MessageMetadata {
    /// How the run that wrote the message ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<RunStatus>,
    /// The first sources collected by the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub evidence: Option<Vec<Citation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub verification: Option<Verification>,
    /// What the message is to its mission, in a mission's conversation
    /// (ADR-089).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mission: Option<MissionPart>,
    /// The inbound hook whose delivery sent the message, as its author's
    /// (ADR-109): its instruction, then what was posted, enclosed.
    #[serde(
        default,
        rename = "inboundHookId",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub inbound_hook_id: Option<Uuid>,
    /// The service its author wrote it from, `slack`: their company's
    /// Slack application (ADR-127).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gateway: Option<String>,
}

/// What a message is to its mission (ADR-089).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MissionPart {
    /// The mission, as its owner entrusted it: its first stage's request.
    Brief,
    /// What a later stage starts from, sent by the platform.
    Resumption,
    /// The note a stage that spent its budget ended with.
    Progress,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub run_id: Option<Uuid>,
    pub metadata: MessageMetadata,
    pub created_at: String,
}

impl Message {
    pub fn from_row(m: &MessageRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: m.id,
            role: stored_text("message role", &m.role)?,
            content: m.content.clone(),
            run_id: m.run_id,
            metadata: stored("message metadata", &m.metadata)?,
            created_at: iso(&m.created_at),
        })
    }
}

/// `GET /conversations/{id}`.
#[derive(Debug, Serialize, TS)]
pub struct ConversationDetail {
    pub conversation: ConversationSummary,
    pub messages: Vec<Message>,
    pub runs: Vec<RunRef>,
}

/// `POST /conversations/{id}/messages`: the run started for the message.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MessageAccepted {
    pub run_id: Uuid,
    pub message_id: Uuid,
}

/// Body of `POST /conversations`.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct NewConversation {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 1, max = 120))]
    #[ts(optional)]
    pub title: Option<String>,
}

/// Body of `PATCH /conversations/{id}`.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct ConversationChanges {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 1, max = 120))]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub archived: Option<bool>,
}

/// Body of `POST /conversations/{id}/messages`.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct NewMessage {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 8000))]
    pub content: String,
}
