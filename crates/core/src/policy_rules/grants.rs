//! What an allow policy may grant (ADR-168). The engine applies allow
//! policies only where each resource is checked (`check`), after the role
//! scope: they open resources outside the scope of whoever already holds
//! the permission somewhere, never the permission itself. An allow policy
//! therefore names only permissions checked resource by resource; on any
//! other it would never apply. It takes two administrators: the one who
//! proposes it and another who approves it, within `GRANT_PROPOSAL_DAYS`.

use super::covered;
use crate::authz::{Enforcement, PermissionInfo};

/// Days a proposal waits for another administrator before it expires.
pub const GRANT_PROPOSAL_DAYS: i64 = 7;

/// Why an allow policy cannot grant these permissions (in French), once
/// they passed `normalize_policy_permissions`: each must be checked
/// resource by resource, every permission a `domaine:*` covers included.
pub fn grant_problem(permissions: &[String], catalog: &[PermissionInfo]) -> Option<String> {
    permissions.iter().find_map(|permission| {
        covered(permission, catalog)
            .iter()
            .find(|p| p.enforcement != Enforcement::PerResource)
            .map(|p| {
                let checked = match p.enforcement {
                    Enforcement::Holder => "est vérifiée sur la personne",
                    _ => "n’est pas encore vérifiée par la plateforme",
                };
                format!(
                    "Une politique d’autorisation ne porte que sur des permissions vérifiées \
                     ressource par ressource ; {} {checked} : l’accès s’y accorde par un rôle",
                    p.permission
                )
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn permission(permission: &'static str, enforcement: Enforcement) -> PermissionInfo {
        PermissionInfo {
            permission,
            label: "",
            delegable: true,
            enforcement,
        }
    }

    const CATALOG: &[PermissionInfo] = &[
        permission("intervention:read", Enforcement::PerResource),
        permission("intervention:write", Enforcement::NotYet),
        permission("approval:decide", Enforcement::PerResource),
        permission("planning:apply", Enforcement::Holder),
    ];

    fn perms(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn grants_only_permissions_checked_resource_by_resource() {
        assert_eq!(
            grant_problem(&perms(&["approval:decide", "intervention:read"]), CATALOG),
            None
        );
        let on_person = grant_problem(&perms(&["planning:apply"]), CATALOG).unwrap();
        assert!(on_person.contains("planning:apply est vérifiée sur la personne"));
    }

    #[test]
    fn a_domain_grants_only_when_all_it_covers_is_checked_per_resource() {
        // intervention:* also covers intervention:write, never checked yet.
        let problem = grant_problem(&perms(&["intervention:*"]), CATALOG).unwrap();
        assert!(problem.contains("intervention:write n’est pas encore vérifiée"));
        assert_eq!(grant_problem(&perms(&["approval:*"]), CATALOG), None);
    }
}
