//! Rules of workspace administration: which permissions only make sense for
//! the whole company, how roles may be assigned, and how the organisation
//! tree may change. Pure functions; the API applies them and audits.

use uuid::Uuid;

use crate::authz::{AssignmentScope, OrgTree, OrgUnitType, matches_permission};

/// Permissions whose screens and actions cover the whole company: they are
/// only honoured, and only assignable, with a company-wide scope. Granted on a
/// single unit they would still expose every unit (audit, connectors…).
pub const TENANT_WIDE_PERMISSIONS: &[&str] = &[
    "org:manage",
    "user:manage",
    "role:manage",
    "policy:manage",
    "connector:manage",
    "skill:manage",
    "settings:manage",
    "memory:manage",
    "audit:read",
    "usage:read",
];

pub fn is_tenant_wide(permission: &str) -> bool {
    TENANT_WIDE_PERMISSIONS.contains(&permission)
}

/// Whether a role (its permission patterns) may only be assigned company-wide.
pub fn requires_tenant_scope(role_permissions: &[String]) -> bool {
    role_permissions.iter().any(|pattern| {
        TENANT_WIDE_PERMISSIONS
            .iter()
            .any(|permission| matches_permission(pattern, permission))
    })
}

/// Why a role assignment is refused, in French; `None` when it is valid.
pub fn assignment_problem(
    role_permissions: &[String],
    scope: &AssignmentScope,
    tree: &OrgTree,
) -> Option<&'static str> {
    match scope {
        AssignmentScope::OrgUnit { org_unit_id, .. } if tree.get(*org_unit_id).is_none() => {
            Some("Unité inconnue")
        }
        AssignmentScope::OrgUnit { .. } | AssignmentScope::Own
            if requires_tenant_scope(role_permissions) =>
        {
            Some("Ce rôle d’administration ne peut être attribué que sur toute l’entreprise")
        }
        _ => None,
    }
}

/// Why `unit` cannot be placed under `new_parent`, in French; `None` when it can.
pub fn move_problem(tree: &OrgTree, unit: Uuid, new_parent: Uuid) -> Option<&'static str> {
    let Some(current) = tree.get(unit) else {
        return Some("Unité inconnue");
    };
    if current.parent_id.is_none() {
        return Some("L’unité racine ne peut pas être déplacée");
    }
    if tree.get(new_parent).is_none() {
        return Some("Unité parente inconnue");
    }
    if tree.is_in_subtree(unit, new_parent) {
        return Some("Une unité ne peut pas être placée sous elle-même ou sous ses sous-unités");
    }
    None
}

/// Why a unit cannot have this type, in French; `None` when it can. The
/// company type belongs to the root, and only to it.
pub fn unit_type_problem(is_root: bool, unit_type: OrgUnitType) -> Option<&'static str> {
    match (is_root, unit_type == OrgUnitType::Company) {
        (true, false) => Some("L’unité racine reste de type « Entreprise »"),
        (false, true) => Some("Le type « Entreprise » est réservé à l’unité racine"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::{self, unit};

    fn perms(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn administration_roles_are_assignable_company_wide_only() {
        let tree = demo::org_tree();
        let admin = perms(&["user:read", "audit:read"]);
        let on_unit = AssignmentScope::subtree(unit::TEAM_NORTH);
        assert!(assignment_problem(&admin, &on_unit, &tree).is_some());
        assert!(assignment_problem(&admin, &AssignmentScope::Own, &tree).is_some());
        assert_eq!(
            assignment_problem(&admin, &AssignmentScope::Tenant, &tree),
            None
        );
        // Wildcards count: `user:*` covers user:manage.
        assert!(requires_tenant_scope(&perms(&["user:*"])));
        assert!(requires_tenant_scope(&perms(&["*"])));

        let business = perms(&["intervention:read", "planning:apply"]);
        assert_eq!(assignment_problem(&business, &on_unit, &tree), None);
        let unknown = AssignmentScope::subtree(uuid::Uuid::new_v4());
        assert_eq!(
            assignment_problem(&business, &unknown, &tree),
            Some("Unité inconnue")
        );
    }

    #[test]
    fn refuses_moves_that_would_break_the_tree() {
        let tree = demo::org_tree();
        assert!(move_problem(&tree, unit::COMPANY, unit::TECHNICAL).is_some());
        assert!(move_problem(&tree, unit::TECHNICAL, unit::TEAM_NORTH).is_some());
        assert!(move_problem(&tree, unit::TECHNICAL, unit::TECHNICAL).is_some());
        assert!(move_problem(&tree, unit::TEAM_NORTH, uuid::Uuid::new_v4()).is_some());
        assert_eq!(move_problem(&tree, unit::TEAM_NORTH, unit::DIRECTION), None);
    }

    #[test]
    fn keeps_the_company_type_for_the_root() {
        assert!(unit_type_problem(false, OrgUnitType::Company).is_some());
        assert!(unit_type_problem(true, OrgUnitType::Team).is_some());
        assert_eq!(unit_type_problem(true, OrgUnitType::Company), None);
        assert_eq!(unit_type_problem(false, OrgUnitType::Team), None);
    }
}
