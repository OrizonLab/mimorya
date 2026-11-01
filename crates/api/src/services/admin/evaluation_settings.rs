//! What the company's skill evaluations may cost over thirty days
//! (ADR-154), set by those who manage its settings (`settings:manage`,
//! company-wide), within what the platform allows, and read by them with
//! what was spent. Those who evaluate the skills do not set it: they would
//! raise the limit they spend under. Each change is audited.

use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::evaluation_settings::{
    remove_evaluation_limit, set_evaluation_limit,
};
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::skill_evaluations::{Spending, checked_limit, spending};

const PERMISSION: &str = "settings:manage";

/// What the company's evaluations cost over thirty days, against their
/// limit.
pub async fn company_evaluation_spending(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<Spending> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let spending = spending(&mut tx).await?;
    tx.commit().await?;
    Ok(spending)
}

/// Sets the company's own limit, if within the platform's bounds.
pub async fn set_company_evaluation_limit(
    db: &Database,
    access: &AccessContext,
    max_cost_micros: i64,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let limit = checked_limit(max_cost_micros)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    set_evaluation_limit(&mut tx, tenant_id, limit, access.subject.user_id).await?;
    append_audit(
        &mut tx,
        audit_entry(
            access,
            "skill.evaluation_limit_configured",
            "tenant",
            tenant_id,
        )
        .metadata(json!({ "maxCostMicros": limit })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Back to the platform's limit.
pub async fn reset_company_evaluation_limit(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if remove_evaluation_limit(&mut tx).await? {
        append_audit(
            &mut tx,
            audit_entry(access, "skill.evaluation_limit_reset", "tenant", tenant_id),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
