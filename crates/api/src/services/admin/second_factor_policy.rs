//! What the company requires of its password accounts (ADR-201), set by
//! those who manage its settings (`settings:manage`, company-wide). A
//! second factor is required only where the server can keep one: without a
//! master key, the requirement is refused and explained. It applies from
//! each account's next sign-in with a password; sessions open stay open.
//! Each change is audited.

use mimorya_core::second_factor::Requirement;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::second_factor_settings::set_second_factor_requirement;
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::secrets::SecretStore;
use crate::services::AccessContext;
use crate::services::second_factor::company_requirement;

const PERMISSION: &str = "settings:manage";

/// The company's requirement.
pub async fn company_second_factor_policy(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<Requirement> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let requirement = company_requirement(&mut tx).await?;
    tx.commit().await?;
    Ok(requirement)
}

/// Sets the company's requirement; a second factor required only where
/// the server can keep one.
pub async fn set_company_second_factor_policy(
    db: &Database,
    secrets: &SecretStore,
    access: &AccessContext,
    requirement: Requirement,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    if requirement != Requirement::Optional {
        secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let before = company_requirement(&mut tx).await?;
    set_second_factor_requirement(
        &mut tx,
        tenant_id,
        requirement.as_str(),
        access.subject.user_id,
    )
    .await?;
    if before != requirement {
        append_audit(
            &mut tx,
            audit_entry(
                access,
                "settings.second_factor_requirement_configured",
                "tenant",
                tenant_id,
            )
            .metadata(json!({
                "requirement": requirement.as_str(),
                "previous": before.as_str(),
            })),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
