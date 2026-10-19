//! Invitation and password reset links: what their page shows before the
//! password is chosen.

use serde::Serialize;
use ts_rs::TS;

use crate::http::iso;
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
