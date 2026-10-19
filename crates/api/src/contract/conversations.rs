//! Conversations of the signed-in user, their messages and the runs they
//! started.

use anyhow::bail;
use mimorya_core::tools::Citation as ToolCitation;
use mimorya_db::repositories::conversations::{ConversationRow, MessageRow};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::runs::{RunRef, RunStatus, Verification};
use super::stored::stored;
use crate::http::iso;

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: Uuid,
    pub title: String,
    pub updated_at: String,
}

impl From<&ConversationRow> for ConversationSummary {
    fn from(c: &ConversationRow) -> Self {
        Self {
            id: c.id,
            title: c.title.clone(),
            updated_at: iso(&c.updated_at),
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
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
        let role = match m.role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            _ => bail!("stored message role does not match the API contract"),
        };
        Ok(Self {
            id: m.id,
            role,
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
