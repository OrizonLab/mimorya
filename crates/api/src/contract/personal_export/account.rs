//! The person's account in their export (ADR-187): who they are, their
//! units and roles, their absences, their sessions and how they sign in.

use mimorya_db::repositories::personal_access::{
    ExportedProvisioning, ExportedSession, ExportedSignIn,
};
use mimorya_db::repositories::personal_data::{
    ExportedAbsence, ExportedProfile, ExportedRole, ExportedUnit,
};
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

/// The account as the company's identity provider made it through SCIM.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportProvisioning {
    pub external_id: Option<String>,
    pub created_at: String,
    pub removed_at: Option<String>,
}

impl ExportAccount {
    pub fn new(
        profile: ExportedProfile,
        units: Vec<ExportedUnit>,
        roles: Vec<ExportedRole>,
    ) -> Self {
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
