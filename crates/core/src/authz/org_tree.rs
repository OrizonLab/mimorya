//! In-memory view of a tenant's organisation (company, sites, departments,
//! teams…) with unlimited depth. TPE/PME trees are small (tens to hundreds of
//! units), so they are loaded per request and subtrees computed in process.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgUnitType {
    Company,
    Site,
    Direction,
    Department,
    Service,
    Team,
}

impl OrgUnitType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::Site => "site",
            Self::Direction => "direction",
            Self::Department => "department",
            Self::Service => "service",
            Self::Team => "team",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Company,
            Self::Site,
            Self::Direction,
            Self::Department,
            Self::Service,
            Self::Team,
        ]
        .into_iter()
        .find(|t| t.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgUnit {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    #[serde(rename = "type")]
    pub unit_type: OrgUnitType,
    /// Organisational lead. Informational only: it grants no access by itself.
    pub lead_user_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrgTreeError {
    #[error("Duplicate org unit {0}")]
    Duplicate(Uuid),
    #[error("Org unit {unit} references unknown parent {parent}")]
    UnknownParent { unit: Uuid, parent: Uuid },
    #[error("Cycle detected in org tree at {0}")]
    Cycle(Uuid),
}

#[derive(Debug, Clone, Default)]
pub struct OrgTree {
    /// Units in their original order.
    units: Vec<OrgUnit>,
    index: HashMap<Uuid, usize>,
    children: HashMap<Uuid, Vec<Uuid>>,
}

impl OrgTree {
    pub fn new(units: Vec<OrgUnit>) -> Result<Self, OrgTreeError> {
        let mut index = HashMap::with_capacity(units.len());
        for (position, unit) in units.iter().enumerate() {
            if index.insert(unit.id, position).is_some() {
                return Err(OrgTreeError::Duplicate(unit.id));
            }
        }
        let mut children: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for unit in &units {
            let Some(parent) = unit.parent_id else {
                continue;
            };
            if !index.contains_key(&parent) {
                return Err(OrgTreeError::UnknownParent {
                    unit: unit.id,
                    parent,
                });
            }
            children.entry(parent).or_default().push(unit.id);
        }
        let tree = Self {
            units,
            index,
            children,
        };
        for unit in &tree.units {
            tree.assert_no_cycle(unit.id)?;
        }
        Ok(tree)
    }

    pub fn get(&self, id: Uuid) -> Option<&OrgUnit> {
        self.index.get(&id).map(|&i| &self.units[i])
    }

    pub fn all(&self) -> &[OrgUnit] {
        &self.units
    }

    pub fn roots(&self) -> impl Iterator<Item = &OrgUnit> {
        self.units.iter().filter(|u| u.parent_id.is_none())
    }

    pub fn children_of(&self, id: Uuid) -> impl Iterator<Item = &OrgUnit> {
        self.children
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|child| self.get(*child))
    }

    /// The unit itself and all of its descendants.
    pub fn subtree(&self, id: Uuid) -> HashSet<Uuid> {
        let mut result = HashSet::new();
        if !self.index.contains_key(&id) {
            return result;
        }
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            result.insert(current);
            stack.extend(self.children.get(&current).into_iter().flatten().copied());
        }
        result
    }

    /// Ancestors from the direct parent up to the root.
    pub fn ancestors(&self, id: Uuid) -> Vec<&OrgUnit> {
        let mut result = Vec::new();
        let mut current = self.get(id);
        while let Some(parent) = current.and_then(|u| u.parent_id).and_then(|p| self.get(p)) {
            result.push(parent);
            current = Some(parent);
        }
        result
    }

    /// Human readable path, e.g. "Entreprise › Service technique › Équipe Nord".
    pub fn path_label(&self, id: Uuid) -> String {
        let Some(unit) = self.get(id) else {
            return String::new();
        };
        let mut names: Vec<&str> = self
            .ancestors(id)
            .iter()
            .rev()
            .map(|u| u.name.as_str())
            .collect();
        names.push(&unit.name);
        names.join(" › ")
    }

    pub fn is_in_subtree(&self, root_id: Uuid, candidate_id: Uuid) -> bool {
        if root_id == candidate_id {
            return self.index.contains_key(&root_id);
        }
        self.ancestors(candidate_id).iter().any(|a| a.id == root_id)
    }

    fn assert_no_cycle(&self, start: Uuid) -> Result<(), OrgTreeError> {
        let mut seen = HashSet::new();
        let mut current = self.get(start);
        while let Some(unit) = current {
            if !seen.insert(unit.id) {
                return Err(OrgTreeError::Cycle(unit.id));
            }
            current = unit.parent_id.and_then(|p| self.get(p));
        }
        Ok(())
    }
}
