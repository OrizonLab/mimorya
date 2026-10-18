//! Workspace administration: accounts, organisation and role assignments.
//! Every operation requires its permission on the whole company, refuses to
//! act on the administrator's own account, runs in one transaction with its
//! audit entry, and refreshes the cached authorization data of the tenant.
//!
//! Changes of one company are serialised (`begin_change`): each one checks
//! its rules on the data as stored, not on the request's cached view, so two
//! administrators acting at once cannot build a cycle in the tree or revoke
//! each other's rights.

mod assignments;
mod org_units;
mod users;

pub use assignments::{grant_role, revoke_role};
pub use org_units::{NewUnitInput, UnitChanges, create_unit, delete_unit, update_unit};
pub use users::{NewUserInput, UserChanges, create_user, reset_password, update_user};

use mimorya_core::DomainError;
use mimorya_core::authz::{Actor, DataScope, OrgTree};
use mimorya_db::repositories::audit::AuditEntry;
use mimorya_db::repositories::org::{load_org_units, load_user_access};
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use crate::services::AccessContext;

/// Opens the transaction of a change under the company's administration lock,
/// once the administrator still holds `permission` company-wide on the stored
/// data (another administrator may have revoked or disabled them meanwhile).
async fn begin_change(
    db: &Database,
    access: &AccessContext,
    permission: &str,
) -> anyhow::Result<TenantTx> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    tx.lock_administration().await?;
    let current = load_user_access(&mut tx, access.subject.user_id).await?;
    let allowed = current
        .filter(|a| a.profile.status == "active")
        .is_some_and(|a| {
            access
                .engine
                .data_scope(&a.subject, permission, &Actor::user())
                == DataScope::All
        });
    if !allowed {
        return Err(DomainError::forbidden(format!(
            "Permission requise sur toute l’entreprise : {permission}"
        ))
        .into());
    }
    Ok(tx)
}

/// The organisation tree as stored now (read under the administration lock).
async fn current_tree(tx: &mut TenantTx) -> anyhow::Result<OrgTree> {
    Ok(OrgTree::new(load_org_units(tx).await?)?)
}

fn audit_entry(
    access: &AccessContext,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
) -> AuditEntry {
    AuditEntry::new(access.subject.tenant_id, "user", action, "success")
        .by_user(access.subject.user_id)
        .resource(resource_type, resource_id)
}

/// Administrators do not change their own account, rights or status: it
/// takes another administrator (no self-escalation, no self-lockout).
fn refuse_self(access: &AccessContext, user_id: Uuid) -> Result<(), DomainError> {
    if access.subject.user_id == user_id {
        return Err(DomainError::forbidden(
            "Vous ne pouvez pas modifier votre propre compte ni vos propres habilitations depuis l’administration",
        ));
    }
    Ok(())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|code| code == "23505")
}
