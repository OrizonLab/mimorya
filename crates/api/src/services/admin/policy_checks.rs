//! What an access policy must pass before it is stored, deny (ADR-022) or
//! allow (ADR-168): permissions an administrator may name, a condition the
//! engine reads, and what each effect may say. An unreadable policy would
//! stop the whole company's authorization from loading; what is stored may
//! also predate a rule, or have been written by hand, hence checked again.

use mimorya_core::DomainError;
use mimorya_core::authz::{Condition, PermissionInfo};
use mimorya_core::policy_rules::{
    canonical_condition, condition_problem, grant_problem, normalize_policy_permissions,
    scope_problem,
};
use serde_json::Value;

pub(super) fn checked_permissions(
    requested: &[String],
    catalog: &[PermissionInfo],
) -> Result<Vec<String>, DomainError> {
    normalize_policy_permissions(requested, catalog).map_err(DomainError::validation)
}

/// The permissions an allow policy grants: those a policy may name, each
/// checked resource by resource.
pub(super) fn checked_grant(
    requested: &[String],
    catalog: &[PermissionInfo],
) -> Result<Vec<String>, DomainError> {
    let permissions = checked_permissions(requested, catalog)?;
    match grant_problem(&permissions, catalog) {
        Some(problem) => Err(DomainError::validation(problem)),
        None => Ok(permissions),
    }
}

pub(super) fn checked_scope(
    permissions: &[String],
    condition: &Condition,
    catalog: &[PermissionInfo],
) -> Result<(), DomainError> {
    match scope_problem(permissions, condition, catalog) {
        Some(problem) => Err(DomainError::validation(problem)),
        None => Ok(()),
    }
}

/// The stored condition, read and checked again (it may predate a rule).
pub(super) fn stored_condition(condition: &Value) -> Result<Condition, DomainError> {
    let refused = || {
        DomainError::validation(
            "La condition enregistrée n’est plus valide : remplacez-la en modifiant la politique",
        )
    };
    let condition: Condition = serde_json::from_value(condition.clone()).map_err(|_| refused())?;
    if condition_problem(&condition).is_some() {
        return Err(refused());
    }
    Ok(condition)
}

/// The condition as it will be stored, once it passed the checks: its text
/// values in canonical form, like the attributes they are compared with.
pub(super) fn checked_condition(condition: &Condition) -> anyhow::Result<Value> {
    if let Some(problem) = condition_problem(condition) {
        return Err(DomainError::validation(problem).into());
    }
    Ok(serde_json::to_value(canonical_condition(
        condition.clone(),
    ))?)
}
