//! The MCP servers each person authorizes for themself (ADR-174), as the
//! signed-in person's account shows them: those they may authorize, and
//! their own authorization of each, never its tokens nor anyone else's.

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use mimorya_db::repositories::connector_person_authorizations::PersonAuthorizationRow;

use crate::http::iso;

/// `GET /account/connector-authorizations`.
#[derive(Debug, Serialize, TS)]
pub struct PersonAuthorizations {
    pub servers: Vec<PersonAuthorizationView>,
}

/// A server the person may authorize, or did.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PersonAuthorizationView {
    pub connector_id: Uuid,
    pub name: String,
    /// An authorization may start: the server is enabled, declared for each
    /// person, and the person may use the company's connectors. Otherwise
    /// their authorization stays theirs to withdraw.
    pub available: bool,
    /// Whether its authorization server must prove that the account they
    /// authorize with is their own, any other refused (ADR-175).
    pub verifies_identity: bool,
    pub authorization: Option<PersonAuthorizationState>,
}

/// The person's own authorization of a server: never its tokens.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PersonAuthorizationState {
    pub authorized_at: String,
    /// When the access token expires, if it does.
    pub expires_at: Option<String>,
    /// When its tokens were last renewed, if they were since it was given.
    pub renewed_at: Option<String>,
    /// Whether a refresh token is kept.
    pub renewable: bool,
    pub scopes: Vec<String>,
    /// Why its last renewal failed, the authorization kept.
    pub renewal_failure: Option<String>,
}

impl From<PersonAuthorizationRow> for PersonAuthorizationState {
    fn from(row: PersonAuthorizationRow) -> Self {
        Self {
            authorized_at: iso(&row.authorized_at),
            expires_at: row.access_expires_at.as_ref().map(iso),
            renewed_at: (row.refreshed_at > row.authorized_at).then(|| iso(&row.refreshed_at)),
            renewable: row.renewable,
            scopes: row.scopes.split_whitespace().map(String::from).collect(),
            renewal_failure: row.renewal_failure,
        }
    }
}
