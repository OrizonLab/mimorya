//! What the person wrote or set up, in their export (ADR-187): their
//! conversations with every message and the summary shown in place of the
//! first ones, their memories, tasks, triggers, missions and deliverables.

use std::collections::HashMap;

use mimorya_db::repositories::personal_activity::ExportedDeliverable;
use mimorya_db::repositories::personal_content::{
    ExportedConversation, ExportedMemory, ExportedMessage, ExportedMission, ExportedSummary,
    ExportedTask, ExportedThread, ExportedTrigger,
};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::iso_opt;
use super::links::ExportThread;
use crate::http::iso;

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportConversation {
    pub id: Uuid,
    pub title: String,
    pub visibility: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    /// What the assistant reads in place of the first messages (ADR-145).
    pub summary: Option<String>,
    /// The messaging thread it is held in (ADR-127, ADR-135, ADR-140).
    pub messaging: Option<ExportThread>,
    pub messages: Vec<ExportMessage>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportMessage {
    /// `user` or `assistant`.
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportMemory {
    pub kind: String,
    pub title: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportTask {
    pub title: String,
    pub instruction: String,
    /// 1 = Monday.
    pub weekdays: Vec<i16>,
    pub time_of_day: String,
    pub time_zone: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportTrigger {
    pub title: String,
    pub instruction: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportMission {
    pub title: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub result: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportDeliverable {
    pub kind: String,
    pub title: String,
    #[ts(type = "unknown")]
    pub content: Value,
    pub status: String,
    pub created_at: String,
}

/// The person's conversations as read: `messages` come conversation by
/// conversation, in the order written.
pub struct ConversationRows {
    pub conversations: Vec<ExportedConversation>,
    pub summaries: Vec<ExportedSummary>,
    pub threads: Vec<ExportedThread>,
    pub messages: Vec<ExportedMessage>,
}

/// The person's conversations, each with its summary, its messaging thread
/// and its messages.
pub fn conversation_exports(rows: ConversationRows) -> Vec<ExportConversation> {
    let mut summaries: HashMap<Uuid, String> = rows
        .summaries
        .into_iter()
        .map(|summary| (summary.conversation_id, summary.content))
        .collect();
    let mut threads: HashMap<Uuid, ExportedThread> = rows
        .threads
        .into_iter()
        .map(|thread| (thread.conversation_id, thread))
        .collect();
    let mut exported: Vec<ExportConversation> = rows
        .conversations
        .into_iter()
        .map(|conversation| ExportConversation {
            summary: summaries.remove(&conversation.id),
            messaging: threads.remove(&conversation.id).map(ExportThread::from),
            id: conversation.id,
            title: conversation.title,
            visibility: conversation.visibility,
            created_at: iso(&conversation.created_at),
            updated_at: iso(&conversation.updated_at),
            archived_at: iso_opt(conversation.archived_at),
            messages: Vec::new(),
        })
        .collect();
    let places: HashMap<Uuid, usize> = exported
        .iter()
        .enumerate()
        .map(|(place, conversation)| (conversation.id, place))
        .collect();
    for message in rows.messages {
        if let Some(&place) = places.get(&message.conversation_id) {
            exported[place].messages.push(ExportMessage {
                role: message.role,
                content: message.content,
                created_at: iso(&message.created_at),
            });
        }
    }
    exported
}

impl From<ExportedMemory> for ExportMemory {
    fn from(memory: ExportedMemory) -> Self {
        Self {
            kind: memory.kind,
            title: memory.title,
            content: memory.content,
            status: memory.status,
            created_at: iso(&memory.created_at),
            expires_at: iso_opt(memory.expires_at),
        }
    }
}

impl From<ExportedTask> for ExportTask {
    fn from(task: ExportedTask) -> Self {
        Self {
            title: task.title,
            instruction: task.instruction,
            weekdays: task.weekdays,
            time_of_day: task.time_of_day,
            time_zone: task.time_zone,
            created_at: iso(&task.created_at),
        }
    }
}

impl From<ExportedTrigger> for ExportTrigger {
    fn from(trigger: ExportedTrigger) -> Self {
        Self {
            title: trigger.title,
            instruction: trigger.instruction,
            enabled: trigger.enabled,
            created_at: iso(&trigger.created_at),
        }
    }
}

impl From<ExportedMission> for ExportMission {
    fn from(mission: ExportedMission) -> Self {
        Self {
            title: mission.title,
            description: mission.description,
            status: mission.status,
            created_at: iso(&mission.created_at),
            finished_at: iso_opt(mission.finished_at),
            result: mission.result,
        }
    }
}

impl From<ExportedDeliverable> for ExportDeliverable {
    fn from(deliverable: ExportedDeliverable) -> Self {
        Self {
            kind: deliverable.kind,
            title: deliverable.title,
            content: deliverable.content,
            status: deliverable.status,
            created_at: iso(&deliverable.created_at),
        }
    }
}
