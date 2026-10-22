//! How the company's memory grows by itself (ADR-076), read and set by those
//! who manage its settings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// `GET /admin/memory-settings`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettingsView {
    /// A lesson is drawn from each run that used tools and proposed in the
    /// personal memory of whom it served.
    pub capture_lessons: bool,
}

/// Body of `PUT /admin/memory-settings`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySettingsUpdate {
    pub capture_lessons: bool,
}
