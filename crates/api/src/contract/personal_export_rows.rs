//! The export of a person's own data made from what the database read for
//! it (ADR-187): instants written as ISO 8601, each conversation with its
//! own messages.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use mimorya_db::repositories::personal_content::{
    ExportedConversation, ExportedMemory, ExportedMessage, ExportedMission, ExportedTask,
    ExportedTrigger,
};
use mimorya_db::repositories::personal_data::{
    ExportedAbsence, ExportedAuthorization, ExportedIdentity, ExportedLink, ExportedProfile,
    ExportedRole, ExportedUnit,
};
use uuid::Uuid;

use super::personal_export::{
    ExportAbsence, ExportAccount, ExportAuthorization, ExportConversation, ExportIdentity,
    ExportLink, ExportMemory, ExportMessage, ExportMission, ExportRole, ExportTask, ExportTrigger,
    ExportUnit,
};
use crate::http::iso;

fn iso_opt(instant: Option<DateTime<Utc>>) -> Option<String> {
    instant.as_ref().map(iso)
}

impl ExportAccount {
    pub fn new(
        profile: ExportedProfile,
        units: Vec<ExportedUnit>,
        roles: Vec<ExportedRole>,
    ) -> Self {
        Self {
            id: profile.id,
            email: profile.email,
            display_name: profile.display_name,
            job_title: profile.job_title,
            status: profile.status,
            clearance: profile.clearance,
            attributes: profile.attributes,
            locale: profile.locale,
            created_at: iso(&profile.created_at),
            last_login_at: iso_opt(profile.last_login_at),
            units: units
                .into_iter()
                .map(|unit| ExportUnit {
                    name: unit.name,
                    primary: unit.is_primary,
                })
                .collect(),
            roles: roles
                .into_iter()
                .map(|role| ExportRole {
                    name: role.name,
                    scope: role.scope_type,
                    unit: role.scope_unit,
                    include_descendants: role.include_descendants,
                    expires_at: iso_opt(role.expires_at),
                })
                .collect(),
        }
    }
}

/// The person's conversations, each with its messages; `messages` come
/// conversation by conversation, in the order they were written.
pub fn conversation_exports(
    conversations: Vec<ExportedConversation>,
    messages: Vec<ExportedMessage>,
) -> Vec<ExportConversation> {
    let mut exported: Vec<ExportConversation> = conversations
        .into_iter()
        .map(|conversation| ExportConversation {
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
    for message in messages {
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

impl From<ExportedAbsence> for ExportAbsence {
    fn from(absence: ExportedAbsence) -> Self {
        Self {
            starts_at: iso(&absence.starts_at),
            ends_at: iso(&absence.ends_at),
            delegate: absence.delegate,
        }
    }
}

impl From<ExportedLink> for ExportLink {
    fn from(link: ExportedLink) -> Self {
        Self {
            connector: link.connector,
            subject: link.subject,
            address: link.address,
            scopes: link.scopes,
            created_at: iso(&link.created_at),
        }
    }
}

impl From<ExportedIdentity> for ExportIdentity {
    fn from(identity: ExportedIdentity) -> Self {
        Self {
            kind: identity.kind,
            gateway: identity.gateway,
            external_id: identity.external_id,
            display_name: identity.display_name,
            address: identity.address,
            created_at: iso(&identity.created_at),
        }
    }
}

impl From<ExportedAuthorization> for ExportAuthorization {
    fn from(authorization: ExportedAuthorization) -> Self {
        Self {
            connector: authorization.connector,
            scopes: authorization.scopes,
            authorized_at: iso(&authorization.authorized_at),
        }
    }
}
