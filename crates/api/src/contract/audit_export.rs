//! What a company's SIEM reads of its audit log with its token (ADR-181):
//! the sealed entries after a place, in the chain's order, each with its
//! place and its seal, which the SIEM keeps as anchors.

use chrono::DateTime;
use mimorya_db::repositories::audit_chain::ChainEntry;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::contract::admin::{AuditActor, AuditOutcome};
use crate::contract::stored_text;
use crate::http::iso;
use crate::services::audit_chain::hex;

/// An entry as exported: its place and seal in the company's chain, then
/// what it records. It names what changed, never a secret or a message's
/// content.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportedAuditEntry {
    pub place: i64,
    /// In hexadecimal.
    pub seal: String,
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
    /// The address the request came from, without the mask of a single
    /// host.
    pub ip: Option<String>,
    pub request_id: Option<String>,
}

/// `GET /audit/export`: the company, its entries after the place asked, and
/// whether more follow the last one.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditExport {
    pub company: Uuid,
    pub entries: Vec<ExportedAuditEntry>,
    pub more: bool,
}

impl ExportedAuditEntry {
    /// A sealed entry as the chain holds it.
    pub fn from_chain(entry: &ChainEntry) -> anyhow::Result<Self> {
        let (Some(place), Some(seal)) = (entry.chain_seq, entry.chain_hash.as_deref()) else {
            anyhow::bail!("audit entry {} exported without its seal", entry.id);
        };
        let occurred = DateTime::from_timestamp_micros(entry.occurred_us)
            .ok_or_else(|| anyhow::anyhow!("audit entry {} has no time", entry.id))?;
        let id = |value: &Option<String>| value.as_deref().map(Uuid::parse_str).transpose();
        Ok(Self {
            place,
            seal: hex(seal),
            id: entry.id,
            occurred_at: iso(&occurred),
            actor_type: stored_text("audit actor", &entry.actor_type)?,
            actor_user_id: id(&entry.actor_user_id)?,
            on_behalf_of_user_id: id(&entry.on_behalf_of_user_id)?,
            run_id: id(&entry.run_id)?,
            action: entry.action.clone(),
            resource_type: entry.resource_type.clone(),
            resource_id: entry.resource_id.clone(),
            outcome: stored_text("audit outcome", &entry.outcome)?,
            reason: entry.reason.clone(),
            metadata: serde_json::from_str(&entry.metadata)?,
            ip: entry.ip.as_deref().map(address),
            request_id: entry.request_id.clone(),
        })
    }
}

/// An address as the database writes it (`192.0.2.7/32`), as a SIEM reads
/// it (`192.0.2.7`): a single host's mask dropped, any other kept.
fn address(stored: &str) -> String {
    match stored.split_once('/') {
        Some((host, "32" | "128")) => host.to_string(),
        _ => stored.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_address_is_exported_without_its_mask() {
        assert_eq!(address("192.0.2.7/32"), "192.0.2.7");
        assert_eq!(address("2001:db8::1/128"), "2001:db8::1");
        assert_eq!(address("192.0.2.0/24"), "192.0.2.0/24");
        assert_eq!(address("192.0.2.7"), "192.0.2.7");
    }
}
