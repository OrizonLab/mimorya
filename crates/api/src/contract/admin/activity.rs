//! What happened on the platform: the audit trail (`audit:read`) and model
//! usage with its cost (`usage:read`).

use mimorya_db::repositories::audit::AuditRow;
use mimorya_db::repositories::usage::UsageSummaryRow;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::EscalationSpending;
use crate::contract::stored_text;
use crate::http::iso;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AuditActor {
    User,
    Agent,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    Denied,
    Failure,
}

/// One entry of the audit trail. It names what changed, never a secret or
/// a message's content.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: i64,
    pub occurred_at: String,
    pub actor_type: AuditActor,
    pub actor_user_id: Option<Uuid>,
    pub on_behalf_of_user_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub outcome: AuditOutcome,
    pub reason: Option<String>,
    #[ts(type = "Record<string, unknown>")]
    pub metadata: Value,
}

impl AuditEntry {
    pub fn from_row(e: &AuditRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: e.id,
            occurred_at: iso(&e.occurred_at),
            actor_type: stored_text("audit actor", &e.actor_type)?,
            actor_user_id: e.actor_user_id,
            on_behalf_of_user_id: e.on_behalf_of_user_id,
            run_id: e.run_id,
            action: e.action.clone(),
            resource_type: e.resource_type.clone(),
            resource_id: e.resource_id.clone(),
            outcome: stored_text("audit outcome", &e.outcome)?,
            reason: e.reason.clone(),
            metadata: e.metadata.clone(),
        })
    }
}

/// `GET /admin/audit`, newest first.
#[derive(Debug, Serialize, TS)]
pub struct AuditLog {
    pub entries: Vec<AuditEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    Llm,
    Embedding,
    Tool,
}

/// The role of a model call (ADR-207); other uses are the main model's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UsageRole {
    Main,
    Escalation,
    Classification,
    Advisor,
    Review,
    /// A question of the decision service (ADR-213).
    Decision,
}

/// Usage of one model, summed over the period.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub provider: String,
    pub model: String,
    pub kind: UsageKind,
    pub role: UsageRole,
    /// Made on the operator's key, the platform's model (ADR-208).
    pub on_operator_key: bool,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// What these calls cost the company, in micro-dollars: their estimated
    /// cost on its own key, the amount the operator re-invoices on its
    /// (ADR-210).
    pub cost_micros: i64,
}

impl UsageRow {
    pub fn from_row(r: &UsageSummaryRow) -> anyhow::Result<Self> {
        Ok(Self {
            provider: r.provider.clone(),
            model: r.model.clone(),
            kind: stored_text("usage kind", &r.kind)?,
            role: stored_text("usage role", &r.role)?,
            on_operator_key: r.on_operator_key,
            calls: r.calls,
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            cost_micros: r.cost_micros,
        })
    }
}

/// `GET /admin/usage`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    /// Start of the period.
    pub since: String,
    pub rows: Vec<UsageRow>,
    /// What the company's model escalation spent this month, against its
    /// budget, if it chose one (ADR-207).
    pub escalation: Option<EscalationSpending>,
    /// Jev's prices, where the operator opens it or the company asked it
    /// (ADR-213).
    pub jev_prices: Vec<JevPrice>,
}

/// A version of Jev's price in USD per million tokens, as read on a date
/// at a source.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JevPrice {
    pub model: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub read_on: String,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use mimorya_agent::llm::ModelRole;
    use mimorya_db::repositories::usage::USAGE_ROLES;

    use super::UsageRole;
    use crate::contract::stored_text;

    #[test]
    fn every_role_the_engine_records_is_stored_and_read_back() {
        let recorded: Vec<&str> = ModelRole::ALL.iter().map(|role| role.as_str()).collect();
        assert_eq!(recorded, USAGE_ROLES);
        for role in USAGE_ROLES {
            stored_text::<UsageRole>("usage role", role).unwrap();
        }
    }
}
