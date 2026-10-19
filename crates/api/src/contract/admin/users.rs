//! The user directory (`user:read`), what account management returns
//! (`user:manage`, `role:manage`) and what it writes. Emails, logins, rights
//! and attributes are only sent to administrators: absent, not `null`, for
//! everyone else.

use mimorya_core::accounts::{DISPLAY_NAME_MAX_CHARS, DISPLAY_NAME_MIN_CHARS, EMAIL_MAX_LEN};
use mimorya_core::authz::Sensitivity as CoreSensitivity;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::assignments::Assignment;
use crate::http::{iso, nullable, trimmed, trimmed_nullable, trimmed_opt};
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

impl From<Sensitivity> for CoreSensitivity {
    fn from(sensitivity: Sensitivity) -> Self {
        match sensitivity {
            Sensitivity::Public => Self::Public,
            Sensitivity::Internal => Self::Internal,
            Sensitivity::Confidential => Self::Confidential,
            Sensitivity::Restricted => Self::Restricted,
        }
    }
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

/// Body of `POST /admin/users`: the account is invited (ADR-027).
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewAccount {
    #[serde(deserialize_with = "trimmed")]
    #[validate(email, length(max = EMAIL_MAX_LEN))]
    pub email: String,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = DISPLAY_NAME_MIN_CHARS, max = DISPLAY_NAME_MAX_CHARS))]
    pub display_name: String,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 120))]
    #[ts(optional)]
    pub job_title: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub org_unit_id: Option<Uuid>,
    /// Absent: `internal`.
    #[serde(default)]
    #[ts(optional)]
    pub clearance: Option<Sensitivity>,
}

/// What an account's status may be set to (an invitation is issued, not set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum StatusChange {
    Active,
    Disabled,
}

/// Body of `PATCH /admin/users/{id}`: only the fields sent change.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountUpdate {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = DISPLAY_NAME_MIN_CHARS, max = DISPLAY_NAME_MAX_CHARS))]
    #[ts(optional)]
    pub display_name: Option<String>,
    /// `null` or blank clears it.
    #[serde(default, deserialize_with = "trimmed_nullable")]
    #[ts(optional)]
    pub job_title: Option<Option<String>>,
    /// `null`: no primary unit.
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional)]
    pub org_unit_id: Option<Option<Uuid>>,
    #[serde(default)]
    #[ts(optional)]
    pub clearance: Option<Sensitivity>,
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<StatusChange>,
    /// Replaces every attribute; `{}` removes them all.
    #[serde(default)]
    #[ts(optional, type = "Record<string, unknown>")]
    pub attributes: Option<Map<String, Value>>,
}
