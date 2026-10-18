//! Rules for the access policies an administrator writes (ADR-022): only
//! restrictive (deny) policies, on business permissions, with a small
//! condition that is checked before it is stored. Allow policies widen access
//! beyond the role scopes, and an administrator could aim one at themselves:
//! they stay out of reach until administration actions need a second approval.

use serde_json::Value;

use crate::administration::TENANT_WIDE_PERMISSIONS;
use crate::authz::{ComparisonOp, Condition, Operand, matches_permission};

pub const MAX_POLICY_PERMISSIONS: usize = 50;
pub const MAX_CONDITION_COMPARISONS: usize = 40;
pub const MAX_CONDITION_DEPTH: usize = 6;
const MAX_ATTRIBUTE_CHARS: usize = 200;

/// Where an attribute path may start (see `authz::conditions`).
pub const ATTRIBUTE_ROOTS: &[&str] = &["subject", "resource", "context"];

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
    ("context.channel", "Canal de la demande"),
];

/// The permissions a policy restricts, trimmed, deduplicated and sorted, or
/// why they are refused (in French). A catalog permission, or `domaine:*` for
/// a domain of the catalog; never an administration permission, which a
/// policy could otherwise use to lock every administrator out.
pub fn normalize_policy_permissions(
    requested: &[String],
    catalog: &[&str],
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
        if !is_catalog_pattern(permission, catalog) {
            return Err(format!("Permission inconnue : {permission}"));
        }
        if TENANT_WIDE_PERMISSIONS
            .iter()
            .any(|admin| matches_permission(permission, admin))
        {
            return Err(format!(
                "Les permissions d’administration ne se règlent pas par politique : {permission}"
            ));
        }
    }
    Ok(permissions)
}

fn is_catalog_pattern(permission: &str, catalog: &[&str]) -> bool {
    if catalog.contains(&permission) {
        return true;
    }
    match permission.strip_suffix(":*") {
        Some(domain) if !domain.is_empty() && !domain.contains([':', '*']) => {
            catalog.iter().any(|p| p.split(':').next() == Some(domain))
        }
        _ => false,
    }
}

/// Why a condition is refused (in French); `None` when it can be stored.
pub fn condition_problem(condition: &Condition) -> Option<String> {
    let mut comparisons = 0;
    check(condition, 1, &mut comparisons)
}

fn check(condition: &Condition, depth: usize, comparisons: &mut usize) -> Option<String> {
    if depth > MAX_CONDITION_DEPTH {
        return Some(format!(
            "Condition trop imbriquée ({MAX_CONDITION_DEPTH} niveaux au plus)"
        ));
    }
    let group = match condition {
        Condition::All(c) => &c.all,
        Condition::Any(c) => &c.any,
        Condition::Not(c) => return check(&c.not, depth + 1, comparisons),
        Condition::Compare(comparison) => {
            *comparisons += 1;
            if *comparisons > MAX_CONDITION_COMPARISONS {
                return Some(format!(
                    "Condition trop longue ({MAX_CONDITION_COMPARISONS} comparaisons au plus)"
                ));
            }
            return comparison_problem(comparison.op, &comparison.left, comparison.right.as_ref());
        }
    };
    if group.is_empty() {
        return Some("Un groupe de conditions ne peut pas être vide".into());
    }
    group.iter().find_map(|c| check(c, depth + 1, comparisons))
}

fn comparison_problem(op: ComparisonOp, left: &Operand, right: Option<&Operand>) -> Option<String> {
    for operand in [Some(left), right].into_iter().flatten() {
        if let Operand::Attr(attr) = operand
            && let Some(problem) = attribute_problem(&attr.attr)
        {
            return Some(problem);
        }
    }
    let is_attr = |o: &Operand| matches!(o, Operand::Attr(_));
    if !is_attr(left) && !right.is_some_and(is_attr) {
        return Some("Une comparaison porte sur au moins un attribut".into());
    }
    match op {
        ComparisonOp::Exists if right.is_some() => {
            Some("« est renseigné » ne prend pas de valeur à comparer".into())
        }
        ComparisonOp::Exists => None,
        _ if right.is_none() => Some("Valeur à comparer manquante".into()),
        ComparisonOp::In | ComparisonOp::NotIn if literal(right).is_some_and(|v| !v.is_array()) => {
            Some("« fait partie de » attend une liste de valeurs".into())
        }
        ComparisonOp::Gt | ComparisonOp::Gte | ComparisonOp::Lt | ComparisonOp::Lte
            if [literal(Some(left)), literal(right)]
                .into_iter()
                .flatten()
                .any(|v| !v.is_number()) =>
        {
            Some("Les comparaisons d’ordre portent sur des nombres".into())
        }
        _ => None,
    }
}

/// The value of a literal operand.
fn literal(operand: Option<&Operand>) -> Option<&Value> {
    match operand {
        Some(Operand::Value(v)) => Some(&v.value),
        _ => None,
    }
}

fn attribute_problem(path: &str) -> Option<String> {
    let mut segments = path.split('.');
    let root_known = segments
        .next()
        .is_some_and(|root| ATTRIBUTE_ROOTS.contains(&root));
    let well_formed = path.len() <= MAX_ATTRIBUTE_CHARS && path.split('.').count() >= 2;
    if !root_known || !well_formed || segments.any(str::is_empty) {
        return Some(format!(
            "Attribut invalide : « {path} » (il commence par subject., resource. ou context.)"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const CATALOG: &[&str] = &[
        "intervention:read",
        "planning:apply",
        "memory:read",
        "memory:manage",
    ];

    fn perms(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_string()).collect()
    }

    fn condition(value: serde_json::Value) -> Condition {
        serde_json::from_value(value).expect("well-formed condition")
    }

    #[test]
    fn restricts_business_permissions_only() {
        assert_eq!(
            normalize_policy_permissions(&perms(&["planning:apply", "intervention:*"]), CATALOG),
            Ok(perms(&["intervention:*", "planning:apply"]))
        );
        assert!(normalize_policy_permissions(&[], CATALOG).is_err());
        assert!(normalize_policy_permissions(&perms(&["*"]), CATALOG).is_err());
        assert!(normalize_policy_permissions(&perms(&["facture:*"]), CATALOG).is_err());
        // memory:* would cover memory:manage, an administration permission.
        assert!(normalize_policy_permissions(&perms(&["memory:*"]), CATALOG).is_err());
        assert!(normalize_policy_permissions(&perms(&["memory:manage"]), CATALOG).is_err());
        assert!(normalize_policy_permissions(&perms(&["memory:read"]), CATALOG).is_ok());
    }

    #[test]
    fn checks_conditions_before_they_are_stored() {
        let department = json!({ "op": "eq", "left": { "attr": "subject.attributes.department" }, "right": { "value": "administratif" } });
        assert_eq!(condition_problem(&condition(department.clone())), None);
        assert_eq!(
            condition_problem(&condition(
                json!({ "all": [department, { "op": "exists", "left": { "attr": "resource.attributes.status" } }] })
            )),
            None
        );
        let refused = [
            json!({ "all": [] }),
            json!({ "op": "eq", "left": { "attr": "user.department" }, "right": { "value": "x" } }),
            json!({ "op": "eq", "left": { "attr": "subject" }, "right": { "value": "x" } }),
            json!({ "op": "eq", "left": { "attr": "subject..x" }, "right": { "value": "x" } }),
            json!({ "op": "eq", "left": { "value": 1 }, "right": { "value": 1 } }),
            json!({ "op": "eq", "left": { "attr": "resource.type" } }),
            json!({ "op": "exists", "left": { "attr": "resource.type" }, "right": { "value": 1 } }),
            json!({ "op": "in", "left": { "attr": "resource.type" }, "right": { "value": "intervention" } }),
            json!({ "op": "gt", "left": { "attr": "resource.sensitivityRank" }, "right": { "value": "2" } }),
        ];
        for value in refused {
            assert!(
                condition_problem(&condition(value.clone())).is_some(),
                "{value}"
            );
        }
        let mut deep = json!({ "op": "exists", "left": { "attr": "resource.type" } });
        for _ in 0..MAX_CONDITION_DEPTH {
            deep = json!({ "not": deep });
        }
        assert!(condition_problem(&condition(deep)).is_some());
        let long = json!({ "any": vec![json!({ "op": "exists", "left": { "attr": "resource.type" } }); MAX_CONDITION_COMPARISONS + 1] });
        assert!(condition_problem(&condition(long)).is_some());
    }
}
