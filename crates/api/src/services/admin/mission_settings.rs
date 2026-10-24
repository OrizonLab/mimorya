//! The company's own mission limits (ADR-093), set by those who manage its
//! settings (`settings:manage`, company-wide), within what the platform
//! allows: how many stages a mission may take and what it may cost. A
//! mission keeps the limits it was entrusted with: a change applies to the
//! next ones. Each change is audited.

use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::mission_settings::{
    MissionSettingsRow, remove_mission_settings, set_mission_settings,
};
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::missions::{MISSION_LIMITS, MissionLimits, company_limits};

const PERMISSION: &str = "settings:manage";

/// The limits of the missions entrusted from now on, and whether they are
/// the company's own.
pub async fn company_mission_limits(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<(MissionLimits, bool)> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let own = company_limits(&mut tx).await?;
    tx.commit().await?;
    Ok(match own {
        Some(limits) => (limits, true),
        None => (MISSION_LIMITS, false),
    })
}

/// Sets the company's own limits, if within the platform's bounds.
pub async fn set_company_mission_limits(
    db: &Database,
    access: &AccessContext,
    max_stages: u32,
    max_cost_micros: i64,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let limits = MissionLimits::checked(max_stages, max_cost_micros)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let row = MissionSettingsRow {
        max_stages: i32::try_from(limits.max_stages)?,
        max_cost_micros: limits.max_cost_micros,
    };
    set_mission_settings(&mut tx, tenant_id, row, access.subject.user_id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "mission.limits_configured", "tenant", tenant_id).metadata(json!({
            "maxStages": limits.max_stages,
            "maxCostMicros": limits.max_cost_micros,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Back to the platform's limits.
pub async fn reset_company_mission_limits(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if remove_mission_settings(&mut tx).await? {
        append_audit(
            &mut tx,
            audit_entry(access, "mission.limits_reset", "tenant", tenant_id),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
