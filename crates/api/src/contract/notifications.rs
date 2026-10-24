//! The e-mails a person chooses to receive from the platform (ADR-094).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// `GET /account/notifications`, and the body of its `PUT`: each kind of
/// e-mail, received or not. Invitations and password resets are always
/// sent.
#[derive(Debug, Serialize, Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationSettings {
    /// The requests awaiting their validation, as a manager or standing in
    /// for one, and the reminders before they expire.
    pub approvals: bool,
    /// Their scheduled tasks' runs and missions' stages awaiting it.
    pub own_runs: bool,
    /// The end of their missions.
    pub mission_ends: bool,
}
