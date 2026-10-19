//! Where a role applies, the roles an account holds, and granting one
//! (`role:manage`, company-wide).

use chrono::{DateTime, Utc};
use mimorya_core::authz::AssignmentScope as CoreScope;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

/// Where a role applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssignmentScope {
    Tenant,
    OrgUnit {
        #[serde(rename = "orgUnitId")]
        org_unit_id: Uuid,
        #[serde(rename = "includeDescendants")]
        include_descendants: bool,
    },
    /// The holder's own data only.
    #[serde(rename = "self")]
    Own,
}

impl From<&CoreScope> for AssignmentScope {
    fn from(scope: &CoreScope) -> Self {
        match scope {
            CoreScope::Tenant => Self::Tenant,
            CoreScope::OrgUnit {
                org_unit_id,
                include_descendants,
            } => Self::OrgUnit {
                org_unit_id: *org_unit_id,
                include_descendants: *include_descendants,
            },
            CoreScope::Own => Self::Own,
        }
    }
}

/// A role held by an account.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub id: Uuid,
    pub role_id: Uuid,
    pub role_name: String,
    pub scope: AssignmentScope,
    /// Role and scope, readable.
    pub label: String,
    pub expires_at: Option<String>,
}

impl From<AssignmentScope> for CoreScope {
    fn from(scope: AssignmentScope) -> Self {
        match scope {
            AssignmentScope::Tenant => Self::Tenant,
            AssignmentScope::OrgUnit {
                org_unit_id,
                include_descendants,
            } => Self::OrgUnit {
                org_unit_id,
                include_descendants,
            },
            AssignmentScope::Own => Self::Own,
        }
    }
}

/// Body of `POST /admin/users/{id}/assignments`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleGrant {
    pub role_id: Uuid,
    pub scope: AssignmentScope,
    /// ISO 8601; absent, the role is held until revoked.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub expires_at: Option<DateTime<Utc>>,
}
