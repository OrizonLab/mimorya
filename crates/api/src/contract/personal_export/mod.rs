//! The export of a person's own data (ADR-187): what the platform keeps
//! about them and what they wrote, set up or had done for them, in their
//! company, as one JSON document they download. Never a password, a token
//! nor a secret (a second factor's included); a colleague's data never.

mod account;
mod activity;
mod content;
mod links;

use chrono::{DateTime, Utc};
use serde::Serialize;
use ts_rs::TS;

pub use account::{AccountRows, ExportAccount, ExportSecondFactor};
pub use activity::run_exports;
pub use content::{ConversationRows, conversation_exports};

use account::{ExportAbsence, ExportInvitation, ExportProvisioning, ExportSession, ExportSignIn};
use activity::{ExportApproval, ExportDecision, ExportRun, ExportUsage};
use content::{
    ExportConversation, ExportDeliverable, ExportMemory, ExportMission, ExportTask, ExportTrigger,
};
use links::{ExportAuthorization, ExportIdentity, ExportLink};

use super::NotificationSettings;
use crate::http::iso;

/// `GET /account/export`: everything exported, as of `exportedAt`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PersonalExport {
    pub exported_at: String,
    pub account: ExportAccount,
    pub notifications: NotificationSettings,
    /// The sessions opened in the person's name, still kept (30 days past
    /// their end, ADR-185).
    pub sessions: Vec<ExportSession>,
    pub sign_ins: Vec<ExportSignIn>,
    pub second_factor: ExportSecondFactor,
    pub provisioning: Vec<ExportProvisioning>,
    /// The invitation still awaiting the person, if any (ADR-027).
    pub invitation: Option<ExportInvitation>,
    pub conversations: Vec<ExportConversation>,
    pub memories: Vec<ExportMemory>,
    pub scheduled_tasks: Vec<ExportTask>,
    pub triggers: Vec<ExportTrigger>,
    pub missions: Vec<ExportMission>,
    pub deliverables: Vec<ExportDeliverable>,
    /// The runs made on the person's behalf, each with the tools it called.
    pub runs: Vec<ExportRun>,
    /// The validations asked of the person for their own runs.
    pub approvals: Vec<ExportApproval>,
    /// The decisions the person gave on others' runs: theirs alone.
    pub decisions: Vec<ExportDecision>,
    pub usage: Vec<ExportUsage>,
    pub absences: Vec<ExportAbsence>,
    pub linked_accounts: Vec<ExportLink>,
    pub messaging_identities: Vec<ExportIdentity>,
    pub server_authorizations: Vec<ExportAuthorization>,
}

impl PersonalExport {
    /// How many messages the export holds, all conversations together.
    pub fn message_count(&self) -> usize {
        self.conversations
            .iter()
            .map(|conversation| conversation.messages.len())
            .sum()
    }
}

fn iso_opt(instant: Option<DateTime<Utc>>) -> Option<String> {
    instant.as_ref().map(iso)
}
