//! The person's account in their export (ADR-187): who they are, their
//! units (those they lead too) and roles, their absences, their sessions,
//! how they sign in, their second factor and the invitation awaiting them.

use mimorya_db::repositories::personal_access::{
    ExportedInvitation, ExportedProvisioning, ExportedSession, ExportedSignIn,
};
use mimorya_db::repositories::personal_data::{
    ExportedAbsence, ExportedProfile, ExportedRole, ExportedUnit,
};
use mimorya_db::repositories::second_factors::SecondFactorState;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::iso_opt;
use crate::http::iso;

/// The person's account, units and roles.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportAccount {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub status: String,
    pub clearance: String,
    /// What access policies read of the person (ADR-023).
    #[ts(type = "Record<string, unknown>")]
    pub attributes: Value,
    pub locale: String,
    pub created_at: String,
    pub last_login_at: Option<String>,
    pub units: Vec<ExportUnit>,
    /// The units the person leads, by name.
    pub leads: Vec<String>,
    pub roles: Vec<ExportRole>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportUnit {
    pub name: String,
    pub primary: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportRole {
    pub name: String,
    /// `tenant`, `org_unit` or `self`.
    pub scope: String,
    pub unit: Option<String>,
    pub include_descendants: bool,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportAbsence {
    pub starts_at: String,
    pub ends_at: String,
    /// Who stood in, by name.
    pub delegate: String,
}

/// The person's second factor (ADR-200): whether it is on, since when, and
/// the recovery codes left. Never its secret nor a code.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportSecondFactor {
    pub enabled: bool,
    pub enabled_at: Option<String>,
    pub recovery_codes_left: i64,
}

impl From<Option<SecondFactorState>> for ExportSecondFactor {
    fn from(state: Option<SecondFactorState>) -> Self {
        match state {
            Some(SecondFactorState {
                enabled_at: Some(enabled_at),
                recovery_codes_left,
            }) => Self {
                enabled: true,
                enabled_at: Some(iso(&enabled_at)),
                recovery_codes_left,
            },
            // An enrolment that waits is no second factor.
            _ => Self {
                enabled: false,
                enabled_at: None,
                recovery_codes_left: 0,
            },
        }
    }
}

/// A session: when, from which address, with what browser.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportSession {
    pub created_at: String,
    pub last_seen_at: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

/// An identity provider the person signs in through.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportSignIn {
    pub provider: String,
    pub kind: String,
    pub subject: String,
    pub created_at: String,
    pub last_used_at: String,
}

/// The invitation still awaiting the person: who sent it, by name.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportInvitation {
    pub invited_by: Option<String>,
    pub created_at: String,
    pub expires_at: String,
}

/// The account as the company's identity provider made it through SCIM.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportProvisioning {
    pub external_id: Option<String>,
    pub created_at: String,
    pub removed_at: Option<String>,
}

/// What the account is made of, as read.
pub struct AccountRows {
    pub profile: ExportedProfile,
    pub units: Vec<ExportedUnit>,
    pub leads: Vec<String>,
    pub roles: Vec<ExportedRole>,
}

impl ExportAccount {
    pub fn new(rows: AccountRows) -> Self {
        let AccountRows {
            profile,
            units,
            leads,
            roles,
        } = rows;
        Self {
            id: profile.id,
            email: profile.email,
            display_name: profile.display_name,
            job_title: profile.job_title,
            status: profile.status,
            clearance: profile.clearance,
            attributes: profile.attributes,
            locale: profile.locale,
            created_at: iso(&profile.created_at),
            last_login_at: iso_opt(profile.last_login_at),
            units: units
                .into_iter()
                .map(|unit| ExportUnit {
                    name: unit.name,
                    primary: unit.is_primary,
                })
                .collect(),
            leads,
            roles: roles
                .into_iter()
                .map(|role| ExportRole {
                    name: role.name,
                    scope: role.scope_type,
                    unit: role.scope_unit,
                    include_descendants: role.include_descendants,
                    expires_at: iso_opt(role.expires_at),
                })
                .collect(),
        }
    }
}

impl From<ExportedAbsence> for ExportAbsence {
    fn from(absence: ExportedAbsence) -> Self {
        Self {
            starts_at: iso(&absence.starts_at),
            ends_at: iso(&absence.ends_at),
            delegate: absence.delegate,
        }
    }
}

impl From<ExportedSession> for ExportSession {
    fn from(session: ExportedSession) -> Self {
        Self {
            created_at: iso(&session.created_at),
            last_seen_at: iso(&session.last_seen_at),
            expires_at: iso(&session.expires_at),
            revoked_at: iso_opt(session.revoked_at),
            ip: session.ip,
            user_agent: session.user_agent,
        }
    }
}

impl From<ExportedSignIn> for ExportSignIn {
    fn from(sign_in: ExportedSignIn) -> Self {
        Self {
            provider: sign_in.provider,
            kind: sign_in.kind,
            subject: sign_in.subject,
            created_at: iso(&sign_in.created_at),
            last_used_at: iso(&sign_in.last_used_at),
        }
    }
}

impl From<ExportedProvisioning> for ExportProvisioning {
    fn from(provisioning: ExportedProvisioning) -> Self {
        Self {
            external_id: provisioning.external_id,
            created_at: iso(&provisioning.created_at),
            removed_at: iso_opt(provisioning.removed_at),
        }
    }
}

impl From<ExportedInvitation> for ExportInvitation {
    fn from(invitation: ExportedInvitation) -> Self {
        Self {
            invited_by: invitation.invited_by,
            created_at: iso(&invitation.created_at),
            expires_at: iso(&invitation.expires_at),
        }
    }
}
