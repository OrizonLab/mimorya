//! Input formats and identifier patterns shared by the demo tools.

use std::sync::LazyLock;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Identifiers of interventions cited in answers (grounding check).
pub const INTERVENTION_ID_PATTERN: &str = r"\bINT-\d{4}\b";

pub static INTERVENTION_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^INT-\d{4}$").expect("valid regex"));
pub static DATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("valid regex"));
pub static TIME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([01]\d|2[0-3]):[0-5]\d$").expect("valid regex"));

/// One slot of a proposed follow-up planning.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProposedSlot {
    #[schemars(regex(pattern = r"^INT-\d{4}$"))]
    #[validate(regex(path = *INTERVENTION_ID_RE))]
    pub intervention_id: String,
    pub technician_id: uuid::Uuid,
    /// AAAA-MM-JJ
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    #[validate(regex(path = *DATE_RE, message = "Format attendu : AAAA-MM-JJ"))]
    pub date: String,
    /// HH:MM
    #[schemars(regex(pattern = r"^([01]\d|2[0-3]):[0-5]\d$"))]
    #[validate(regex(path = *TIME_RE, message = "Format attendu : HH:MM"))]
    pub start_time: String,
    #[validate(range(min = 0.5, max = 8.0))]
    pub duration_hours: f64,
    #[validate(length(min = 3, max = 300))]
    pub reason: String,
}
