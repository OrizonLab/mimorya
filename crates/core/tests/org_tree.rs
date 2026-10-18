//! Organisation tree of unlimited depth.

use std::collections::HashSet;

use mimorya_core::authz::{OrgTree, OrgTreeError, OrgUnit, OrgUnitType};
use mimorya_core::demo::{self, unit};
use uuid::Uuid;

fn team(id: Uuid, parent_id: Option<Uuid>, name: &str) -> OrgUnit {
    OrgUnit {
        id,
        parent_id,
        name: name.into(),
        unit_type: OrgUnitType::Team,
        lead_user_id: None,
    }
}

#[test]
fn computes_subtrees_of_unlimited_depth() {
    let tree = demo::org_tree();
    assert_eq!(
        tree.subtree(unit::TECHNICAL),
        HashSet::from([unit::TECHNICAL, unit::TEAM_NORTH, unit::TEAM_SOUTH])
    );
    assert_eq!(tree.subtree(unit::COMPANY).len(), 6);
    assert!(tree.subtree(Uuid::new_v4()).is_empty());
}

#[test]
fn lists_ancestors_from_parent_to_root_and_builds_a_readable_path() {
    let tree = demo::org_tree();
    let ancestors: Vec<Uuid> = tree
        .ancestors(unit::TEAM_NORTH)
        .iter()
        .map(|u| u.id)
        .collect();
    assert_eq!(ancestors, vec![unit::TECHNICAL, unit::COMPANY]);
    assert_eq!(
        tree.path_label(unit::TEAM_NORTH),
        "Néo Maintenance › Service technique › Équipe Nord"
    );
}

#[test]
fn answers_subtree_membership() {
    let tree = demo::org_tree();
    assert!(tree.is_in_subtree(unit::TECHNICAL, unit::TEAM_SOUTH));
    assert!(!tree.is_in_subtree(unit::TEAM_SOUTH, unit::TECHNICAL));
    assert!(tree.is_in_subtree(unit::ADMIN, unit::ADMIN));
}

#[test]
fn supports_deep_hierarchies() {
    let ids: Vec<Uuid> = (0..50).map(|_| Uuid::new_v4()).collect();
    let units = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            team(
                *id,
                i.checked_sub(1).map(|p| ids[p]),
                &format!("Niveau {i}"),
            )
        })
        .collect();
    let deep = OrgTree::new(units).unwrap();
    assert_eq!(deep.subtree(ids[10]).len(), 40);
    assert_eq!(deep.ancestors(ids[49]).len(), 49);
}

#[test]
fn rejects_unknown_parents_duplicates_and_cycles() {
    let (a, b, x) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    assert!(matches!(
        OrgTree::new(vec![team(a, Some(x), "A")]),
        Err(OrgTreeError::UnknownParent { .. })
    ));
    assert!(matches!(
        OrgTree::new(vec![team(a, None, "A"), team(a, None, "A")]),
        Err(OrgTreeError::Duplicate(_))
    ));
    assert!(matches!(
        OrgTree::new(vec![team(a, Some(b), "A"), team(b, Some(a), "B")]),
        Err(OrgTreeError::Cycle(_))
    ));
}
