//! Rules for the access policies an administrator writes (ADR-022): only
//! restrictive (deny) policies, on business permissions the platform actually
//! checks, with a condition checked before it is stored. Allow policies widen
//! access beyond the role scopes, and an administrator could aim one at
//! themselves: they stay out of reach until administration actions need a
//! second approval.
//!
//! Where a deny applies (`authz::engine`): a condition about the person or the
//! actor holds wherever the permission is checked; a condition reading the
//! resource only where each resource is checked (`Enforcement::PerResource`).

mod condition;

pub use condition::{
    ATTRIBUTE_ROOTS, MAX_CONDITION_COMPARISONS, MAX_CONDITION_DEPTH, canonical_condition,
    condition_problem,
};

use crate::administration::TENANT_WIDE_PERMISSIONS;
use crate::authz::{Condition, Enforcement, PermissionInfo, matches_permission, reads_resource};

pub const MAX_POLICY_PERMISSIONS: usize = 50;

/// What the administration screens read: never restricted by a policy, which
/// could otherwise leave administrators unable to see accounts or units.
const ADMINISTRATION_READS: &[&str] = &["org:read", "user:read"];

/// Attributes every evaluation provides, with their French label; free keys
/// can also be read under `subject.attributes.`, `resource.attributes.` and
/// `context.`.
pub const KNOWN_ATTRIBUTES: &[(&str, &str)] = &[
    ("subject.userId", "Identifiant du collaborateur"),
    ("subject.orgUnitIds", "Unités du collaborateur"),
    (
        "subject.clearanceRank",
        "Habilitation du collaborateur (0 à 3)",
    ),
    ("resource.type", "Type de la ressource"),
    ("resource.ownerUserId", "Propriétaire de la ressource"),
    ("resource.orgUnitId", "Unité de la ressource"),
    (
        "resource.sensitivityRank",
        "Sensibilité de la ressource (0 à 3)",
    ),
    ("context.actorType", "Acteur : « user » ou « agent »"),
    (
        "context.channel",
        "Canal : « web », « hook » (déclencheur) ou « evaluation » (compétence évaluée)",
    ),
];

/// The catalog permissions a permission or a `domaine:*` pattern covers.
fn covered<'a>(pattern: &str, catalog: &'a [PermissionInfo]) -> Vec<&'a PermissionInfo> {
    let well_formed = match pattern.strip_suffix(":*") {
        Some(domain) => !domain.is_empty() && !domain.contains([':', '*']),
        None => !pattern.contains('*'),
    };
    if !well_formed {
        return Vec::new();
    }
    catalog
        .iter()
        .filter(|p| matches_permission(pattern, p.permission))
        .collect()
}

/// The permissions a policy restricts, trimmed, deduplicated and sorted, or
/// why they are refused (in French): a catalog permission or `domaine:*`,
/// never an administration permission (a deny could lock administrators out)
/// nor what administration screens read, and only what the platform checks.
pub fn normalize_policy_permissions(
    requested: &[String],
    catalog: &[PermissionInfo],
) -> Result<Vec<String>, String> {
    let mut permissions: Vec<String> = requested.iter().map(|p| p.trim().to_string()).collect();
    permissions.sort();
    permissions.dedup();
    if permissions.is_empty() {
        return Err("Une politique porte sur au moins une permission".into());
    }
    if permissions.len() > MAX_POLICY_PERMISSIONS {
        return Err(format!(
            "Une politique porte sur au plus {MAX_POLICY_PERMISSIONS} permissions"
        ));
    }
    for permission in &permissions {
        let covered = covered(permission, catalog);
        if covered.is_empty() {
            return Err(format!("Permission inconnue : {permission}"));
        }
        if covered.iter().any(|p| {
            TENANT_WIDE_PERMISSIONS.contains(&p.permission)
                || ADMINISTRATION_READS.contains(&p.permission)
        }) {
            return Err(format!(
                "Les permissions d’administration, et celles que lisent ses écrans, ne se règlent \
                 pas par politique : {permission}"
            ));
        }
        if covered.iter().all(|p| p.enforcement == Enforcement::NotYet) {
            return Err(format!(
                "Permission pas encore vérifiée par la plateforme, une politique n’y aurait aucun \
                 effet : {permission}"
            ));
        }
    }
    Ok(permissions)
}

/// Why this condition cannot restrict these permissions (in French): a
/// condition reading the resource only applies where each resource is checked.
pub fn scope_problem(
    permissions: &[String],
    condition: &Condition,
    catalog: &[PermissionInfo],
) -> Option<String> {
    if !reads_resource(condition) {
        return None;
    }
    permissions.iter().find_map(|permission| {
        covered(permission, catalog)
            .iter()
            .any(|p| p.enforcement == Enforcement::Holder)
            .then(|| {
                format!(
                    "Une condition sur la ressource ne s’applique qu’aux permissions vérifiées \
                     ressource par ressource ; {permission} est vérifiée sur la personne : \
                     conditionnez-la sur le collaborateur ou le contexte"
                )
            })
    })
}

/// Whether a policy may restrict this permission (or pattern) at all.
pub fn is_restrictable(permission: &str, catalog: &[PermissionInfo]) -> bool {
    normalize_policy_permissions(&[permission.to_string()], catalog).is_ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

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
        permission("planning:apply", Enforcement::Holder),
        permission("memory:read", Enforcement::Holder),
        permission("memory:manage", Enforcement::Holder),
        permission("mission:read", Enforcement::NotYet),
        permission("user:read", Enforcement::Holder),
    ];

    fn perms(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn restricts_checked_business_permissions_only() {
        assert_eq!(
            normalize_policy_permissions(&perms(&["planning:apply", "intervention:*"]), CATALOG),
            Ok(perms(&["intervention:*", "planning:apply"]))
        );
        assert!(normalize_policy_permissions(&[], CATALOG).is_err());
        for refused in [
            "*",
            "facture:*",
            "facture:read",
            "intervention:**",
            // memory:* would cover memory:manage, an administration permission.
            "memory:*",
            "memory:manage",
            "user:read",
            // Not checked anywhere yet: a deny would have no effect.
            "mission:read",
            "intervention:write",
        ] {
            assert!(
                normalize_policy_permissions(&perms(&[refused]), CATALOG).is_err(),
                "{refused}"
            );
        }
        assert!(is_restrictable("memory:read", CATALOG));
    }

    #[test]
    fn conditions_on_the_resource_need_per_resource_checks() {
        let on_resource: Condition = serde_json::from_value(json!({
            "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "cancelled" }
        }))
        .unwrap();
        let on_actor: Condition = serde_json::from_value(json!({
            "op": "eq", "left": { "attr": "context.actorType" }, "right": { "value": "agent" }
        }))
        .unwrap();
        assert_eq!(
            scope_problem(&perms(&["intervention:*"]), &on_resource, CATALOG),
            None
        );
        assert!(scope_problem(&perms(&["planning:apply"]), &on_resource, CATALOG).is_some());
        assert_eq!(
            scope_problem(&perms(&["planning:apply"]), &on_actor, CATALOG),
            None
        );
    }

    #[test]
    fn a_resource_read_anywhere_in_the_condition_needs_per_resource_checks() {
        let person = json!({ "op": "eq", "left": { "attr": "subject.attributes.department" }, "right": { "value": "administratif" } });
        for reading in [
            json!({ "op": "neq", "left": { "attr": "subject.userId" }, "right": { "attr": "resource.ownerUserId" } }),
            json!({ "all": [person, { "not": { "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "completed" } } }] }),
            json!({ "any": [person, { "op": "eq", "left": { "attr": "resource.type" }, "right": { "value": "intervention" } }] }),
            json!({ "op": "gte", "left": { "attr": "resource.sensitivityRank" }, "right": { "value": 2 } }),
        ] {
            let condition: Condition = serde_json::from_value(reading.clone()).unwrap();
            assert!(
                scope_problem(&perms(&["planning:apply"]), &condition, CATALOG).is_some(),
                "{reading}"
            );
            assert_eq!(
                scope_problem(&perms(&["intervention:read"]), &condition, CATALOG),
                None,
                "{reading}"
            );
        }
    }

    #[test]
    fn a_domain_checked_partly_on_the_person_refuses_resource_conditions() {
        const MIXED: &[PermissionInfo] = &[
            permission("contrat:read", Enforcement::PerResource),
            permission("contrat:export", Enforcement::Holder),
        ];
        let on_resource: Condition = serde_json::from_value(json!({
            "op": "exists", "left": { "attr": "resource.attributes.status" }
        }))
        .unwrap();
        assert!(scope_problem(&perms(&["contrat:*"]), &on_resource, MIXED).is_some());
        assert_eq!(
            scope_problem(&perms(&["contrat:read"]), &on_resource, MIXED),
            None
        );
    }
}
