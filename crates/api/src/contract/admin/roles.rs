//! Roles and the permission catalog (`role:manage`), custom roles as the
//! administration writes them, and the company's skills (`skill:manage`).

use mimorya_core::authz::{Enforcement as CoreEnforcement, PermissionInfo};
use mimorya_db::repositories::roles::RoleRow;
use mimorya_db::repositories::skills::SkillRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::contract::stored::stored_text;
use crate::http::{iso, trimmed, trimmed_opt};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RoleView {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    /// Provided by the platform: neither changed nor deleted.
    pub is_system: bool,
    pub permissions: Vec<String>,
    /// Assignments of the role, expired ones included.
    pub assignments: i64,
    pub held_by_you: bool,
}

impl RoleView {
    pub fn new(role: &RoleRow, held_by_you: bool) -> Self {
        Self {
            id: role.id,
            key: role.key.clone(),
            name: role.name.clone(),
            description: role.description.clone(),
            is_system: role.is_system,
            permissions: role.permissions.clone(),
            assignments: role.assignments,
            held_by_you,
        }
    }
}

/// Where a permission is checked so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    /// Declared, not checked anywhere yet.
    NotYet,
    /// Checked on the person holding it and on the acting agent.
    Holder,
    /// Also checked on each resource.
    PerResource,
}

impl From<CoreEnforcement> for Enforcement {
    fn from(enforcement: CoreEnforcement) -> Self {
        match enforcement {
            CoreEnforcement::NotYet => Self::NotYet,
            CoreEnforcement::Holder => Self::Holder,
            CoreEnforcement::PerResource => Self::PerResource,
        }
    }
}

#[derive(Debug, Serialize, TS)]
pub struct PermissionView {
    pub permission: String,
    pub label: String,
    /// `false`: the agent can never exercise it.
    pub delegable: bool,
    pub enforcement: Enforcement,
}

impl From<&PermissionInfo> for PermissionView {
    fn from(p: &PermissionInfo) -> Self {
        Self {
            permission: p.permission.into(),
            label: p.label.into(),
            delegable: p.delegable,
            enforcement: p.enforcement.into(),
        }
    }
}

/// `GET /admin/roles`.
#[derive(Debug, Serialize, TS)]
pub struct Roles {
    pub roles: Vec<RoleView>,
    pub catalog: Vec<PermissionView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Draft,
    Active,
    Retired,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkillView {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: SkillStatus,
    pub active_version: Option<i32>,
    pub updated_at: String,
}

impl SkillView {
    pub fn from_row(s: &SkillRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: s.id,
            name: s.name.clone(),
            description: s.description.clone(),
            status: stored_text("skill status", &s.status)?,
            active_version: s.active_version,
            updated_at: iso(&s.updated_at),
        })
    }
}

/// `GET /admin/skills`.
#[derive(Debug, Serialize, TS)]
pub struct SkillList {
    pub skills: Vec<SkillView>,
}

/// Body of `POST /admin/roles`: a custom role and its permissions.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRole {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    /// Absent: none.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 500))]
    #[ts(optional)]
    pub description: Option<String>,
    #[validate(length(max = 100))]
    pub permissions: Vec<String>,
}

/// Body of `PATCH /admin/roles/{id}`: only the fields sent change.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleUpdate {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 2, max = 80))]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 500))]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[validate(length(max = 100))]
    #[ts(optional)]
    pub permissions: Option<Vec<String>>,
}
