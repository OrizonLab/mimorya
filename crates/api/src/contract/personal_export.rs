//! The export of a person's own data (ADR-187): what the platform keeps
//! about them and what they wrote or set up, in their company, as one JSON
//! document they download. Never a password, a token nor a secret; a
//! colleague's data never.

use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::NotificationSettings;

/// `GET /account/export`: everything exported, as of `exportedAt`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PersonalExport {
    pub exported_at: String,
    pub account: ExportAccount,
    pub notifications: NotificationSettings,
    pub conversations: Vec<ExportConversation>,
    pub memories: Vec<ExportMemory>,
    pub scheduled_tasks: Vec<ExportTask>,
    pub triggers: Vec<ExportTrigger>,
    pub missions: Vec<ExportMission>,
    pub absences: Vec<ExportAbsence>,
    pub linked_accounts: Vec<ExportLink>,
    pub messaging_identities: Vec<ExportIdentity>,
    pub server_authorizations: Vec<ExportAuthorization>,
}

/// The person's account, units and roles.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportAccount {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub status: String,
    pub clearance: String,
    /// What access policies read of the person (ADR-023).
    #[ts(type = "Record<string, unknown>")]
    pub attributes: Value,
    pub locale: String,
    pub created_at: String,
    pub last_login_at: Option<String>,
    pub units: Vec<ExportUnit>,
    pub roles: Vec<ExportRole>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportUnit {
    pub name: String,
    pub primary: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportRole {
    pub name: String,
    /// `tenant` or `org_unit`.
    pub scope: String,
    pub unit: Option<String>,
    pub include_descendants: bool,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportConversation {
    pub id: Uuid,
    pub title: String,
    pub visibility: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
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
pub struct ExportAbsence {
    pub starts_at: String,
    pub ends_at: String,
    /// Who stood in, by name.
    pub delegate: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportLink {
    pub connector: String,
    /// Who the provider says the account is.
    pub subject: String,
    pub address: String,
    pub scopes: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportIdentity {
    /// `slack`, `telegram` or `teams`.
    pub kind: String,
    pub gateway: String,
    /// The person's identifier in the messaging service.
    pub external_id: String,
    pub display_name: Option<String>,
    pub address: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportAuthorization {
    pub connector: String,
    pub scopes: String,
    pub authorized_at: String,
}
