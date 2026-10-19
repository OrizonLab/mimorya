//! Password reset links (ADR-028): an active account that forgot its
//! password asks for a one-time link, e-mailed to its address, and chooses a
//! new password there. Asking reveals nothing (same answer, link issued in
//! the background); the link works once, for an hour, and ends every
//! session of the account.

mod complete;
mod message;
mod request;

use std::time::Duration;

pub use complete::{ResetView, complete_reset, inspect_reset};
pub use message::{ResetMail, reset_mail};
pub use request::{ResetRequest, ResetSender, send_reset_link};

/// How long a link works.
pub const RESET_VALIDITY: Duration = Duration::from_secs(3600);

/// The token travels in the fragment: browsers send it neither to the
/// server with the page nor in `Referer`.
pub fn reset_link(web_origin: &str, token: &str) -> String {
    format!("{web_origin}/reset-password#{token}")
}
