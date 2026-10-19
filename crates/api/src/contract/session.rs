//! The signed-in user's session: who they are, their company, what they may
//! do.

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

/// `GET /auth/me`.
#[derive(Debug, Serialize, TS)]
pub struct Me {
    pub user: MeUser,
    pub tenant: MeTenant,
    /// Roles held, readable: "Responsable d’équipe — Service technique et sous-unités".
    pub grants: Vec<String>,
    /// Permissions of the catalog held somewhere.
    pub permissions: Vec<String>,
    pub assistant: AssistantStatus,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MeUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    pub org_unit: Option<OrgUnitRef>,
    /// Temporary password: nothing else is reachable until it is replaced.
    pub must_change_password: bool,
}

#[derive(Debug, Serialize, TS)]
pub struct OrgUnitRef {
    pub id: Uuid,
    /// Its place in the organisation, from the root.
    pub path: String,
}

#[derive(Debug, Serialize, TS)]
pub struct MeTenant {
    pub id: Uuid,
    pub name: String,
    /// The demonstration company (fictitious data).
    pub demo: bool,
}

#[derive(Debug, Serialize, TS)]
pub struct AssistantStatus {
    pub name: String,
    /// Whether the company's runs have a model provider they can use.
    pub available: bool,
}

/// `GET /auth/options`: what the sign-in page may offer.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuthOptions {
    /// An e-mail relay is configured: "forgotten password" can send a link.
    pub password_reset: bool,
}
