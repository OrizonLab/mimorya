//! Whether a lesson is drawn from the company's runs (ADR-076), set by those
//! who manage its settings (`settings:manage`, company-wide): rereading a
//! run sends it to the model once more. Each change is audited.

use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::memory_settings::{captures_lessons, set_capture_lessons};
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::services::AccessContext;

const PERMISSION: &str = "settings:manage";

pub async fn lesson_capture(db: &Database, access: &AccessContext) -> anyhow::Result<bool> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let enabled = captures_lessons(&mut tx).await?;
    tx.commit().await?;
    Ok(enabled)
}

pub async fn set_lesson_capture(
    db: &Database,
    access: &AccessContext,
    enabled: bool,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    set_capture_lessons(&mut tx, tenant_id, enabled, access.subject.user_id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "memory.lessons_configured", "tenant", tenant_id)
            .metadata(json!({ "captureLessons": enabled })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
