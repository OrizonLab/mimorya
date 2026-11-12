//! Whether an account must have a second factor to sign in with a password
//! (ADR-201): the company's requirement, and for its administrators,
//! whether the account holds a permission that administers it, in any
//! scope. An invited account's roles count before it accepts: its first
//! sign-in is its acceptance.

use mimorya_core::second_factor::{ADMINISTRATION_PERMISSIONS, Requirement, administers};
use mimorya_db::Database;
use mimorya_db::repositories::second_factor_settings::second_factor_requirement;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::services::{AccessContext, AccessService};

/// The company's requirement, no one's without one set.
pub async fn company_requirement(conn: &mut PgConnection) -> sqlx::Result<Requirement> {
    let stored = second_factor_requirement(conn).await?;
    Ok(stored
        .as_deref()
        .and_then(Requirement::parse)
        .unwrap_or_default())
}

/// Whether `user_id` must have a second factor; read in its own
/// transactions, before any lock.
pub async fn second_factor_required(
    db: &Database,
    access: &AccessService,
    tenant_id: Uuid,
    user_id: Uuid,
) -> anyhow::Result<bool> {
    let mut tx = db.begin(tenant_id).await?;
    let requirement = company_requirement(&mut tx).await?;
    tx.commit().await?;
    Ok(match requirement {
        Requirement::Administrators => {
            access
                .holds_any(tenant_id, user_id, &ADMINISTRATION_PERMISSIONS)
                .await?
        }
        other => other.applies(false),
    })
}

/// Whether the signed-in person must have one.
pub async fn required_of(db: &Database, access: &AccessContext) -> anyhow::Result<bool> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let requirement = company_requirement(&mut tx).await?;
    tx.commit().await?;
    Ok(requirement.applies(administers(|permission| access.has(permission))))
}
