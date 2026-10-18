//! Workspace administration: accounts, organisation and role assignments.
//! Every operation requires its permission on the whole company, refuses to
//! act on the administrator's own account, runs in one transaction with its
//! audit entry, and refreshes the cached authorization data of the tenant.

mod assignments;
mod org_units;
mod users;

pub use assignments::{grant_role, revoke_role};
pub use org_units::{NewUnitInput, UnitChanges, create_unit, delete_unit, update_unit};
pub use users::{NewUserInput, UserChanges, create_user, reset_password, update_user};

use mimorya_core::DomainError;
use mimorya_db::repositories::audit::AuditEntry;
use uuid::Uuid;

use crate::services::AccessContext;

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
