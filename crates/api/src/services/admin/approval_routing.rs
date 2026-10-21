//! Which risks a company has its managers validate (ADR-059), set by those
//! who manage its access policies (`policy:manage`, company-wide). Each
//! change is audited with what it replaced.

use mimorya_core::autonomy::ToolRisk;
use mimorya_db::Database;
use mimorya_db::repositories::approval_settings::{get_manager_risks, put_manager_risks};
use mimorya_db::repositories::audit::append_audit;
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::services::AccessContext;

const PERMISSION: &str = "policy:manage";

pub async fn manager_risks(db: &Database, access: &AccessContext) -> anyhow::Result<Vec<String>> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let risks = get_manager_risks(&mut tx).await?;
    tx.commit().await?;
    Ok(risks)
}

pub async fn set_manager_risks(
    db: &Database,
    access: &AccessContext,
    risks: &[ToolRisk],
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    // Once each, from the lightest to the heaviest.
    let chosen: Vec<&str> = ToolRisk::ALL
        .into_iter()
        .filter(|risk| risks.contains(risk))
        .map(ToolRisk::as_str)
        .collect();
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let previous = get_manager_risks(&mut tx).await?;
    put_manager_risks(&mut tx, tenant_id, &chosen, access.subject.user_id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "approval_routing.updated", "tenant", tenant_id).metadata(json!({
            "managerRisks": chosen,
            "previousManagerRisks": previous,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
