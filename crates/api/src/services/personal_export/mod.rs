//! A person's export of their own data (ADR-187): what the platform keeps
//! about them and what they wrote, set up or had done for them, in their
//! company, read in one snapshot of it, for them alone. Never a password,
//! a token nor a secret; never a colleague's data. A process makes a few
//! at once, one at a time for a person: an export reads every message they
//! wrote, and its place is held until the file is sent or dropped. The
//! export is traced in the company's audit log by its counts alone, once
//! read and written, before anything is sent: no trace, no file.

mod file;
mod reading;

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;
use uuid::Uuid;

use crate::plugins::RequestContext;
use crate::services::AccessContext;
use crate::services::places::{Place, Places};

pub use file::ExportFile;

/// Exports one process makes at once.
pub const EXPORTS_AT_ONCE: usize = 2;

const BUSY: &str = "D’autres exports de données sont en cours : réessayez dans un instant.";
const EXPORTING: &str = "L’export de vos données est déjà en cours : attendez qu’il se termine.";

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

/// The signed-in person's own data, as the file they download, traced in
/// their company's audit log. The file holds the export's place until it
/// is sent or dropped.
pub async fn export_own_data(
    db: &Database,
    exports: &Exports,
    access: &AccessContext,
    request: &RequestContext,
) -> anyhow::Result<ExportFile> {
    let place = exports.take(access.subject.user_id)?;
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut snapshot = db.begin_snapshot(tenant_id).await?;
    let export = reading::read_export(&mut snapshot, user_id).await?;
    snapshot.commit().await?;
    let file = ExportFile::new(&export, place)?;
    let counts = json!({
        "conversations": export.conversations.len(),
        "messages": export.message_count(),
        "memories": export.memories.len(),
        "runs": export.runs.len(),
    });
    // Written: what was read is not kept twice while the trace is.
    drop(export);
    let mut entry = AuditEntry::new(tenant_id, "user", "account.exported", "success")
        .by_user(user_id)
        .resource("user", user_id)
        .ip(request.ip_string())
        .metadata(counts);
    entry.request_id = Some(request.id.to_string());
    let mut tx = db.begin(tenant_id).await?;
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(file)
}
