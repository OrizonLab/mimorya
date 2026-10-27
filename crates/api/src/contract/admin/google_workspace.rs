//! A company's Google Workspace client as the administration declares and
//! reads it (`connector:manage`, ADR-121): its OAuth client ID, the address
//! to register in it, and how many people linked their account. The secret
//! is never returned, only whether one is stored.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::trimmed;

/// Body of `POST /admin/connectors/google-workspace`: the OAuth client
/// declared in the company's Google Cloud project, with delegated
/// permissions.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewGoogleWorkspaceConnector {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    /// The OAuth client ID, ending with `.apps.googleusercontent.com`.
    #[serde(deserialize_with = "trimmed")]
    pub client_id: String,
    #[serde(deserialize_with = "trimmed")]
    pub client_secret: String,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
    /// Whether its tools may send e-mail and create events in the person's
    /// name, each validated. Absent: they only read.
    #[serde(default)]
    #[ts(optional)]
    pub write_tools: Option<bool>,
}

/// A company's Google Workspace client, as `ConnectorView` shows it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GoogleWorkspaceApp {
    pub client_id: String,
    /// The address to register as the client's authorised redirect URI.
    pub redirect_uri: String,
    pub secret_stored: bool,
    /// How many people linked their account.
    pub links: i64,
    /// Its tools may send e-mail and create events.
    pub write_tools: bool,
}
