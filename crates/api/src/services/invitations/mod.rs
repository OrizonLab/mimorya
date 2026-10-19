//! Invitation links (ADR-027): the e-mail that carries them, and what they
//! open before sign-in. Administrators issue them (`services::admin`).

mod accept;
mod message;

use std::time::Duration;

pub use accept::{InvitationView, accept_invitation, inspect_invitation};
pub use message::{InvitationMail, invitation_mail};

/// How long a link works.
pub const INVITATION_VALIDITY: Duration = Duration::from_secs(7 * 24 * 3600);

/// The token travels in the fragment: browsers send it neither to the
/// server with the page nor in `Referer`.
pub fn invitation_link(web_origin: &str, token: &str) -> String {
    format!("{web_origin}/invitation#{token}")
}
