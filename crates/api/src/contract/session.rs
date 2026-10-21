//! The signed-in user's session: who they are, their company, what they may
//! do; signing in and changing one's password.

use mimorya_core::accounts::EMAIL_MAX_LEN;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::auth::credentials::MIN_PASSWORD_CHARS;
use crate::http::trimmed;

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
    /// False when the account signs in through its company's identity
    /// provider only (ADR-064): it has no password to change.
    pub has_password: bool,
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

/// Body of `POST /auth/login`.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    #[serde(deserialize_with = "trimmed")]
    #[validate(email, length(max = EMAIL_MAX_LEN))]
    pub email: String,
    #[validate(length(min = 1, max = 512))]
    pub password: String,
}

/// Body of `POST /account/password`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordUpdate {
    #[validate(length(min = 1, max = 512))]
    pub current_password: String,
    #[validate(length(min = "MIN_PASSWORD_CHARS", max = 512))]
    pub new_password: String,
}
