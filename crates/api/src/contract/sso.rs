//! Signing in through the company's identity provider (ADR-063): whether
//! an address may, and where to go.

use mimorya_core::accounts::EMAIL_MAX_LEN;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
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
}

/// `POST /auth/sso/discover`.
#[derive(Debug, Serialize, TS)]
pub struct SsoDiscovery {
    /// Absent: the password only.
    pub provider: Option<SsoProvider>,
}

/// `POST /auth/sso/start`: where the browser goes to sign in.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SsoStart {
    pub redirect_url: String,
}
