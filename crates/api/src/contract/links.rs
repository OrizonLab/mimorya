//! Invitation and password reset links: asking for a reset link, what a
//! link's page shows, and the password chosen with it. The token travels in
//! the request body, never in a URL.

use mimorya_core::accounts::EMAIL_MAX_LEN;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::auth::credentials::MIN_PASSWORD_CHARS;
use crate::http::{iso, trimmed};
use crate::services::invitations::InvitationView;
use crate::services::password_reset::ResetView;

/// `POST /auth/invitation`: who the invitation is for.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InvitationDetails {
    pub email: String,
    pub display_name: String,
    pub company: String,
    pub expires_at: String,
}

impl From<&InvitationView> for InvitationDetails {
    fn from(invitation: &InvitationView) -> Self {
        Self {
            email: invitation.email.clone(),
            display_name: invitation.display_name.clone(),
            company: invitation.company.clone(),
            expires_at: iso(&invitation.expires_at),
        }
    }
}

/// `POST /auth/password-reset/link`: the account whose password is reset.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResetDetails {
    pub email: String,
    pub expires_at: String,
}

impl From<&ResetView> for ResetDetails {
    fn from(reset: &ResetView) -> Self {
        Self {
            email: reset.email.clone(),
            expires_at: iso(&reset.expires_at),
        }
    }
}

/// Body of `POST /auth/password-reset`: the address to send a link to.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct ResetLinkRequest {
    #[serde(deserialize_with = "trimmed")]
    #[validate(email, length(max = EMAIL_MAX_LEN))]
    pub email: String,
}

/// Body of `POST /auth/invitation` and `POST /auth/password-reset/link`: the
/// link to open.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct LinkToken {
    #[validate(length(min = 1, max = 200))]
    pub token: String,
}

/// Body of `POST /auth/invitation/accept` and `POST /auth/password-reset/complete`:
/// the password chosen with a link.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct LinkPassword {
    #[validate(length(min = 1, max = 200))]
    pub token: String,
    #[validate(length(min = "MIN_PASSWORD_CHARS", max = 512))]
    pub password: String,
}
