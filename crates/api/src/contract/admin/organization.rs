//! The organisation tree as the administration shows it (`org:read`), and
//! its changes (`org:manage`).

use mimorya_core::authz::{OrgUnit, OrgUnitType as CoreUnitType};
use mimorya_db::repositories::users::DirectoryEntry;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::{nullable, trimmed, trimmed_opt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OrgUnitType {
    Company,
    Site,
    Direction,
    Department,
    Service,
    Team,
}

impl From<CoreUnitType> for OrgUnitType {
    fn from(unit_type: CoreUnitType) -> Self {
        match unit_type {
            CoreUnitType::Company => Self::Company,
            CoreUnitType::Site => Self::Site,
            CoreUnitType::Direction => Self::Direction,
            CoreUnitType::Department => Self::Department,
            CoreUnitType::Service => Self::Service,
            CoreUnitType::Team => Self::Team,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrgUnitView {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    #[serde(rename = "type")]
    pub unit_type: OrgUnitType,
    /// Organisational lead: informational, it grants no access by itself.
    pub lead_user_id: Option<Uuid>,
    /// Accounts whose primary unit it is.
    pub members: Vec<Uuid>,
}

impl OrgUnitView {
    pub fn new(unit: &OrgUnit, members: Vec<Uuid>) -> Self {
        Self {
            id: unit.id,
            parent_id: unit.parent_id,
            name: unit.name.clone(),
            unit_type: unit.unit_type.into(),
            lead_user_id: unit.lead_user_id,
            members,
        }
    }
}

/// An account as the organisation screen names it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrgPerson {
    pub id: Uuid,
    pub display_name: String,
    pub job_title: Option<String>,
}

impl From<&DirectoryEntry> for OrgPerson {
    fn from(u: &DirectoryEntry) -> Self {
        Self {
            id: u.id,
            display_name: u.display_name.clone(),
            job_title: u.job_title.clone(),
        }
    }
}

/// `GET /admin/organization`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub units: Vec<OrgUnitView>,
    pub users: Vec<OrgPerson>,
    /// Changing the tree requires `org:manage` on the whole company.
    pub can_manage: bool,
}

/// Body of `POST /admin/org-units`. The type is checked by the route, which
/// names an unknown one in French.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewOrgUnit {
    pub parent_id: Uuid,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 120))]
    pub name: String,
    #[serde(rename = "type")]
    #[ts(as = "OrgUnitType")]
    pub unit_type: String,
}

/// Body of `PATCH /admin/org-units/{id}`: only the fields sent change.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrgUnitUpdate {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 1, max = 120))]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    #[ts(optional, as = "Option<OrgUnitType>")]
    pub unit_type: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub parent_id: Option<Uuid>,
    /// `null`: no lead.
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional)]
    pub lead_user_id: Option<Option<Uuid>>,
}
