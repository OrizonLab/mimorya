//! The company's identity providers (`settings:manage`, ADR-063): declared,
//! changed, removed. The client secret is write-only: never returned, only
//! whether one is stored.

use mimorya_db::repositories::identity_providers::IdentityProviderRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::{iso, trimmed, trimmed_opt};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdentityProviderView {
    pub id: Uuid,
    /// `microsoft`, `google` or `oidc`.
    pub kind: String,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    /// E-mail domains whose accounts sign in through it.
    pub domains: Vec<String>,
    pub enabled: bool,
    /// Its domains' accounts sign in through it alone, but for the
    /// company's administrators of its sign-in (ADR-066).
    pub required: bool,
    pub secret_stored: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl IdentityProviderView {
    pub fn new(row: IdentityProviderRow, secret_stored: bool) -> Self {
        Self {
            id: row.id,
            created_at: iso(&row.created_at),
            updated_at: iso(&row.updated_at),
            kind: row.kind,
            name: row.name,
            issuer: row.issuer,
            client_id: row.client_id,
            domains: row.domains,
            enabled: row.enabled,
            required: row.required,
            secret_stored,
        }
    }
}

/// `GET /admin/identity-providers`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdentityProvidersView {
    /// The server can seal client secrets (`MIMORYA_MASTER_KEY`).
    pub secrets_available: bool,
    /// The address to register at the provider for the return of a sign-in.
    pub redirect_uri: String,
    pub providers: Vec<IdentityProviderView>,
}

/// Body of `POST /admin/identity-providers`. What designates the provider
/// depends on its kind: the directory (tenant ID) of a Microsoft Entra ID,
/// nothing for Google, the issuer's address otherwise.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewIdentityProvider {
    #[serde(deserialize_with = "trimmed")]
    pub kind: String,
    /// Absent or blank: the kind's name ("Microsoft 365", "Google Workspace").
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub directory_id: Option<String>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub issuer: Option<String>,
    #[serde(deserialize_with = "trimmed")]
    pub client_id: String,
    pub client_secret: String,
    pub domains: Vec<String>,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
    /// Absent: not required (ADR-066).
    #[serde(default)]
    #[ts(optional)]
    pub required: Option<bool>,
}

/// Body of `PATCH /admin/identity-providers/{id}`: absent fields are
/// unchanged. Kind, issuer and client are not: another provider is declared.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityProviderUpdate {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub name: Option<String>,
    /// A new secret replaces the stored one.
    #[serde(default)]
    #[ts(optional)]
    pub client_secret: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub domains: Option<Vec<String>>,
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub required: Option<bool>,
}
