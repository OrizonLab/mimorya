//! The user directory (`user:read`) and what account management returns
//! (`user:manage`, `role:manage`). Emails, logins, rights and attributes are
//! only sent to administrators: absent, not `null`, for everyone else.

use mimorya_core::authz::{AssignmentScope as CoreScope, Sensitivity as CoreSensitivity};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::http::iso;
use crate::services::admin::IssuedInvitation as Issued;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    /// Created, its invitation link not used yet.
    Invited,
    Disabled,
}

/// Highest sensitivity of the data an account may read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

impl From<CoreSensitivity> for Sensitivity {
    fn from(sensitivity: CoreSensitivity) -> Self {
        match sensitivity {
            CoreSensitivity::Public => Self::Public,
            CoreSensitivity::Internal => Self::Internal,
            CoreSensitivity::Confidential => Self::Confidential,
            CoreSensitivity::Restricted => Self::Restricted,
        }
    }
}

/// Where a role applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
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

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryUser {
    pub id: Uuid,
    pub display_name: String,
    pub job_title: Option<String>,
    pub status: AccountStatus,
    /// Its primary unit, as a path from the root.
    pub org_unit: Option<String>,
    /// To account or role managers only.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_login_at: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub org_unit_id: Option<Option<Uuid>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub clearance: Option<Sensitivity>,
    /// To account managers only: read by access policies as
    /// `subject.attributes.*` (ADR-023).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub attributes: Option<Value>,
    /// To account managers only: when the pending invitation link expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub invitation_expires_at: Option<Option<String>>,
    /// To role managers only.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub grants: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignments: Option<Vec<Assignment>>,
}

/// Account and role management, each requiring its permission company-wide.
#[derive(Debug, Serialize, TS)]
pub struct DirectoryRights {
    pub users: bool,
    pub roles: bool,
}

/// `GET /admin/users`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    pub users: Vec<DirectoryUser>,
    pub can_manage: DirectoryRights,
    /// Invitations and reset links go by e-mail.
    pub email_enabled: bool,
}

/// An invitation link just issued: e-mailed, or to transmit.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IssuedInvitation {
    pub expires_at: String,
    pub emailed: bool,
    /// Shown once, when no e-mail went out.
    pub link: Option<String>,
    /// Why no e-mail went out.
    pub problem: Option<String>,
}

impl From<&Issued> for IssuedInvitation {
    fn from(invitation: &Issued) -> Self {
        Self {
            expires_at: iso(&invitation.expires_at),
            emailed: invitation.emailed,
            link: invitation.link.clone(),
            problem: invitation.problem.clone(),
        }
    }
}

/// `POST /admin/users`.
#[derive(Debug, Serialize, TS)]
pub struct UserCreated {
    pub id: Uuid,
    pub invitation: IssuedInvitation,
}

/// `POST /admin/users/{id}/invitation`: a new link; the former one stops working.
#[derive(Debug, Serialize, TS)]
pub struct InvitationIssued {
    pub invitation: IssuedInvitation,
}

/// `POST /admin/users/{id}/password`: shown once, replaced at next sign-in.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TemporaryPassword {
    pub temporary_password: String,
}

/// `POST /admin/users/{id}/password-link`: the administrator never sees the link.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResetLinkSent {
    pub expires_at: String,
}
