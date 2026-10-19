//! Invitation links (ADR-027): storing a new one, the e-mail that carries
//! them, and what they open before sign-in. Administrators issue them
//! (`services::admin`), as does the platform's operator for an account no
//! administrator can invite (`services::provisioning`).

mod accept;
mod message;
mod renew;

use std::time::Duration;

pub use accept::{InvitationView, accept_invitation, inspect_invitation};
pub use message::{InvitationMail, invitation_mail};
pub use renew::{RenewedInvitation, renew_invitation};

/// How long a link works.
pub const INVITATION_VALIDITY: Duration = Duration::from_secs(7 * 24 * 3600);

/// The token travels in the fragment: browsers send it neither to the
/// server with the page nor in `Referer`.
pub fn invitation_link(web_origin: &str, token: &str) -> String {
    format!("{web_origin}/invitation#{token}")
}
