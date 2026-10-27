//! A company's Microsoft 365 application as the administration declares
//! and reads it (`connector:manage`, ADR-116): its directory and client IDs,
//! the address to register in it, and how many people linked their
//! account. The secret is never returned, only whether one is stored.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::trimmed;

/// Body of `POST /admin/connectors/microsoft365`: the application declared
/// in the company's Microsoft Entra directory, with delegated permissions.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewMicrosoft365Connector {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    /// The directory (tenant) ID, a GUID.
    #[serde(deserialize_with = "trimmed")]
    pub directory: String,
    /// The application (client) ID, a GUID.
    #[serde(deserialize_with = "trimmed")]
    pub client_id: String,
    #[serde(deserialize_with = "trimmed")]
    pub client_secret: String,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

/// A company's Microsoft 365 application, as `ConnectorView` shows it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Microsoft365App {
    pub directory: Uuid,
    pub client_id: String,
    /// The address to register as the application's redirect URI (web).
    pub redirect_uri: String,
    pub secret_stored: bool,
    /// How many people linked their account.
    pub links: i64,
}
