//! Signing in through the company's identity provider (ADR-063): whether
//! an address may, or which provider the company's sign-in link names
//! (ADR-067), and where to go.

use mimorya_core::accounts::EMAIL_MAX_LEN;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::trimmed;

/// Body of `POST /auth/sso/discover` and `POST /auth/sso/start`.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct SsoLookup {
    #[serde(deserialize_with = "trimmed")]
    #[validate(email, length(max = EMAIL_MAX_LEN))]
    pub email: String,
}

/// A provider the sign-in page offers.
#[derive(Debug, Serialize, TS)]
pub struct SsoProvider {
    /// `microsoft`, `google` or `oidc`.
    pub kind: String,
    /// As the button names it: "Microsoft 365".
    pub name: String,
    /// The account signs in through it alone, unless it administers the
    /// company's sign-in (ADR-066).
    pub required: bool,
}

/// `POST /auth/sso/discover`.
#[derive(Debug, Serialize, TS)]
pub struct SsoDiscovery {
    /// Absent: the password only.
    pub provider: Option<SsoProvider>,
}

/// Body of `POST /auth/sso/link` and `POST /auth/sso/link/start`: the
/// provider the company's sign-in link names.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct SsoLinkLookup {
    pub provider: Uuid,
}

/// `POST /auth/sso/link`: the company the link signs in to, and its provider.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SsoLink {
    pub company: String,
    pub provider: SsoProvider,
    /// A first sign-in creates the account of a person of its domains.
    pub creates_accounts: bool,
}

/// `POST /auth/sso/start` and `POST /auth/sso/link/start`: where the
/// browser goes to sign in.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SsoStart {
    pub redirect_url: String,
}
