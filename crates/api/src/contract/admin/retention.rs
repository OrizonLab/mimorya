//! How long the company keeps its idle conversations, its episodes and the
//! personal memories of its accounts gone (ADR-195), read and set by those
//! who hold `retention:manage` company-wide, within the platform's bounds.

use chrono::{DateTime, Utc};
use mimorya_core::retention::{RetentionKind, RetentionUnit};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// What retention purges, by the duration a company sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RetentionKindName {
    Conversations,
    Episodes,
    Departed,
}

impl From<RetentionKind> for RetentionKindName {
    fn from(kind: RetentionKind) -> Self {
        match kind {
            RetentionKind::Conversations => Self::Conversations,
            RetentionKind::Episodes => Self::Episodes,
            RetentionKind::Departed => Self::Departed,
        }
    }
}

/// The unit of a duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RetentionUnitName {
    Months,
    Days,
}

impl From<RetentionUnit> for RetentionUnitName {
    fn from(unit: RetentionUnit) -> Self {
        match unit {
            RetentionUnit::Months => Self::Months,
            RetentionUnit::Days => Self::Days,
        }
    }
}

/// One kind's duration.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RetentionDurationView {
    pub kind: RetentionKindName,
    /// What it is, in French.
    pub label: String,
    pub unit: RetentionUnitName,
    /// The company's choice, or the platform's without one.
    pub duration: i32,
    /// What the purge applies now: longer than `duration` while a
    /// shortening waits.
    pub in_effect: i32,
    /// Whether the company chose it.
    pub own: bool,
    /// When a shortening that waits applies; absent when none waits.
    #[ts(optional, type = "string")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortening_applies_at: Option<DateTime<Utc>>,
    /// The platform's duration, which applies without the company's choice.
    pub platform: i32,
    /// What a company may set: from this much…
    pub min: i32,
    /// …to this much.
    pub max: i32,
}

/// `GET /admin/retention`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RetentionView {
    pub durations: Vec<RetentionDurationView>,
    /// How many days a shortening waits before it applies.
    pub shortening_delay_days: i32,
    /// The version of the durations the company set, which a change names
    /// (`RetentionUpdate::replaces`, ADR-196).
    pub version: String,
}

/// Body of `PUT /admin/retention`: every kind's duration, checked against
/// the platform's bounds by the service, which says them in its refusal.
/// One unchanged is left as it is, a shortening that waits with it.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetentionUpdate {
    pub conversation_months: i32,
    pub episode_months: i32,
    pub departed_days: i32,
    /// The `version` the durations were read at: refused once they changed
    /// since, what the change was asked on no longer being there.
    #[validate(length(min = 1, max = 64))]
    pub replaces: String,
}

impl RetentionUpdate {
    /// Each kind with the duration asked for it.
    pub fn durations(&self) -> [(RetentionKind, i32); 3] {
        [
            (RetentionKind::Conversations, self.conversation_months),
            (RetentionKind::Episodes, self.episode_months),
            (RetentionKind::Departed, self.departed_days),
        ]
    }
}
