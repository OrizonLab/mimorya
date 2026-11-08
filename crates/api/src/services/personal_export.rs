//! A person's export of their own data (ADR-187): what the platform keeps
//! about them and what they wrote or set up, in their company, read in one
//! transaction of it, for them alone. Never a password, a token nor a
//! secret; never a colleague's data. A process makes a few at once, one
//! at a time for a person: an export reads every message they wrote. The
//! export is traced in the company's audit log by its counts alone.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::notification_settings::notification_settings;
use mimorya_db::repositories::personal_content::{
    exported_conversations, exported_memories, exported_messages, exported_missions,
    exported_tasks, exported_triggers,
};
use mimorya_db::repositories::personal_data::{
    exported_absences, exported_authorizations, exported_identities, exported_links,
    exported_profile, exported_roles, exported_units,
};
use serde_json::json;
use uuid::Uuid;

use crate::contract::{ExportAccount, NotificationSettings, PersonalExport, conversation_exports};
use crate::http::iso;
use crate::services::AccessContext;
use crate::services::places::{Place, Places};

/// Exports one process makes at once.
pub const EXPORTS_AT_ONCE: usize = 2;

const BUSY: &str = "D’autres exports de données sont en cours : réessayez dans un instant.";
const EXPORTING: &str = "L’export de vos données est déjà en cours : attendez qu’il se termine.";
const NO_ACCOUNT: &str = "Compte introuvable";

/// The places of the exports this process makes, and their people.
#[derive(Clone)]
pub struct Exports(Places);

impl Default for Exports {
    fn default() -> Self {
        Self(Places::new(EXPORTS_AT_ONCE, BUSY, EXPORTING))
    }
}

impl Exports {
    /// A place for an export of `user_id`'s data; refused while they have
    /// one, or while this process makes as many as it may.
    pub fn take(&self, user_id: Uuid) -> Result<Place, DomainError> {
        self.0.take(user_id)
    }
}

/// The signed-in person's own data, traced in their company's audit log.
pub async fn export_own_data(
    db: &Database,
    exports: &Exports,
    access: &AccessContext,
) -> anyhow::Result<PersonalExport> {
    let _place = exports.take(access.subject.user_id)?;
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    let profile = exported_profile(&mut tx, user_id)
        .await?
        .ok_or_else(|| DomainError::not_found(NO_ACCOUNT))?;
    let units = exported_units(&mut tx, user_id).await?;
    let roles = exported_roles(&mut tx, user_id).await?;
    let chosen = notification_settings(&mut tx, user_id).await?;
    let messages = exported_messages(&mut tx, user_id).await?;
    let message_count = messages.len();
    let conversations =
        conversation_exports(exported_conversations(&mut tx, user_id).await?, messages);
    let export = PersonalExport {
        exported_at: iso(&Utc::now()),
        account: ExportAccount::new(profile, units, roles),
        notifications: NotificationSettings {
            approvals: chosen.approvals,
            own_runs: chosen.own_runs,
            mission_ends: chosen.mission_ends,
        },
        conversations,
        memories: into_all(exported_memories(&mut tx, user_id).await?),
        scheduled_tasks: into_all(exported_tasks(&mut tx, user_id).await?),
        triggers: into_all(exported_triggers(&mut tx, user_id).await?),
        missions: into_all(exported_missions(&mut tx, user_id).await?),
        absences: into_all(exported_absences(&mut tx, user_id).await?),
        linked_accounts: into_all(exported_links(&mut tx, user_id).await?),
        messaging_identities: into_all(exported_identities(&mut tx, user_id).await?),
        server_authorizations: into_all(exported_authorizations(&mut tx, user_id).await?),
    };
    let entry = AuditEntry::new(tenant_id, "user", "account.exported", "success")
        .by_user(user_id)
        .resource("user", user_id)
        .metadata(json!({
            "conversations": export.conversations.len(),
            "messages": message_count,
            "memories": export.memories.len(),
        }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(export)
}

fn into_all<R, T: From<R>>(rows: Vec<R>) -> Vec<T> {
    rows.into_iter().map(T::from).collect()
}
