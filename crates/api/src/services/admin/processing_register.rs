//! The company's record of processing activities (ADR-217), as who holds
//! `compliance:read` company-wide reads it: drawn from its settings as they
//! stand, in one transaction of its own, each time it is asked for, and
//! stored nowhere. A model is named where the company's data goes now: the
//! one its runs resolve to, the operator's on its key when its formula
//! supplies it (ADR-208), nothing for a choice that cannot be used.

use mimorya_agent::decision::{DecisionMode, DecisionUsage};
use mimorya_agent::llm::embedding::embedding_uses_listed_endpoint;
use mimorya_agent::llm::uses_listed_endpoint;
use mimorya_connectors::standard_mail::{MAIL_SERVER, MailServerConfig};
use mimorya_core::identity_providers::ProviderKind;
use mimorya_core::processing_register::{
    ChosenBy, ConnectorFact, ConnectorKind, GatewayKind, IdentityFact, JevUses, ModelService,
    Register, RegisterFacts, RetentionFact, register,
};
use mimorya_core::retention::RetentionKind;
use mimorya_core::second_factor::Requirement;
use mimorya_db::Database;
use mimorya_db::repositories::bearer_tokens::{TokenTable, count_tokens};
use mimorya_db::repositories::connectors::{ConnectorInstanceRow, list_connector_instances};
use mimorya_db::repositories::decision_settings::decision_usages;
use mimorya_db::repositories::embedding_settings::get_embedding_settings;
use mimorya_db::repositories::gateways::list_gateways;
use mimorya_db::repositories::identity_providers::list_identity_providers;
use mimorya_db::repositories::llm_settings::{LlmSettingsRow, ModelPurpose, get_llm_settings};
use mimorya_db::repositories::retention_durations::{StoredRetention, retention_durations};
use mimorya_db::repositories::second_factor_settings::second_factor_requirement;
use url::Url;

use super::retention::RetentionDuration;
use crate::services::embeddings::EmbeddingResolution;
use crate::services::llm::{Resolution, Review};
use crate::services::{AccessContext, EmbeddingService, LlmService};

const PERMISSION: &str = "compliance:read";

/// What the register reads besides the database.
pub struct RegisterSources<'a> {
    pub db: &'a Database,
    pub llm: &'a LlmService,
    pub embeddings: &'a EmbeddingService,
    /// The platform sends e-mails: a relay is configured.
    pub mails: bool,
}

pub async fn processing_register(
    sources: RegisterSources<'_>,
    access: &AccessContext,
) -> anyhow::Result<Register> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let models = sources.llm.resolve(tenant_id).await?;
    let embedder = sources.embeddings.resolve(tenant_id).await?;
    let jev_opens = sources.llm.jev_opens(tenant_id).await?;

    let mut tx = sources.db.begin(tenant_id).await?;
    let runs_row = get_llm_settings(&mut tx, ModelPurpose::Conversations).await?;
    let review_row = get_llm_settings(&mut tx, ModelPurpose::Verification).await?;
    let usages = decision_usages(&mut tx).await?;
    let connectors = list_connector_instances(&mut tx, true).await?;
    let gateways = list_gateways(&mut tx).await?;
    let providers = list_identity_providers(&mut tx).await?;
    let scim = count_tokens(&mut tx, TokenTable::Scim).await? > 0;
    let audit_export = count_tokens(&mut tx, TokenTable::AuditExport).await? > 0;
    let stored = retention_durations(&mut tx).await?;
    let second_factor = second_factor_requirement(&mut tx).await?;
    // Read with the rest: the model the embedder serves must be the one
    // stored now.
    let embedding_row = get_embedding_settings(&mut tx).await?;
    tx.commit().await?;

    let runs_model = match &models.runs {
        Resolution::Company(_) => runs_row.as_ref().map(chosen_model),
        Resolution::Platform(Some(_)) => {
            sources
                .llm
                .platform_endpoint()
                .map(|(kind, base_url)| ModelService {
                    provider: kind.to_string(),
                    listed_host: listed_host(uses_listed_endpoint(kind), base_url),
                    chosen_by: ChosenBy::Operator,
                })
        }
        Resolution::Platform(None) | Resolution::Refused(_) => None,
    };
    let review_model = match &models.review {
        Review::Company(_) => review_row.as_ref().map(chosen_model),
        Review::Runs | Review::Refused(_) => None,
    };
    let embeddings = match (embedder.as_ref(), embedding_row) {
        (EmbeddingResolution::Ready(embedder), Some(row)) if embedder.serves(&row) => {
            Some(ModelService {
                listed_host: listed_host(
                    embedding_uses_listed_endpoint(&row.provider),
                    row.base_url.as_deref(),
                ),
                provider: row.provider,
                chosen_by: ChosenBy::Company,
            })
        }
        _ => None,
    };
    let facts = RegisterFacts {
        company: access.tenant.name.clone(),
        runs_model,
        review_model,
        embeddings,
        jev: if jev_opens {
            jev_uses(
                usages
                    .iter()
                    .map(|row| (row.usage.as_str(), row.mode.as_str())),
            )
        } else {
            JevUses::default()
        },
        connectors: connectors.iter().map(connector).collect(),
        gateways: gateways
            .iter()
            .filter(|gateway| gateway.enabled)
            .filter_map(|gateway| match gateway.kind.as_str() {
                "slack" => Some(GatewayKind::Slack),
                "telegram" => Some(GatewayKind::Telegram),
                "teams" => Some(GatewayKind::Teams),
                _ => None,
            })
            .collect(),
        identity_providers: providers
            .iter()
            .filter(|provider| provider.enabled)
            .filter_map(|provider| {
                Some(IdentityFact {
                    kind: ProviderKind::parse(&provider.kind)?,
                    name: provider.name.clone(),
                    host: host(&provider.issuer),
                })
            })
            .collect(),
        scim,
        audit_export,
        mails: sources.mails,
        retention: RetentionKind::ALL
            .into_iter()
            .map(|kind| retention(kind, &stored))
            .collect(),
        second_factor: second_factor
            .as_deref()
            .and_then(Requirement::parse)
            .unwrap_or_default(),
    };
    Ok(register(&facts))
}

/// A model the company chose, as stored.
fn chosen_model(row: &LlmSettingsRow) -> ModelService {
    ModelService {
        provider: row.provider.clone(),
        listed_host: listed_host(uses_listed_endpoint(&row.provider), row.base_url.as_deref()),
        chosen_by: ChosenBy::Company,
    }
}

/// The host of an endpoint the operator lists, for a kind served there.
fn listed_host(listed: bool, base_url: Option<&str>) -> Option<String> {
    listed.then(|| base_url.and_then(host)).flatten()
}

/// The host of an address, if it is one.
fn host(address: &str) -> Option<String> {
    Url::parse(address)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
}

/// The activities Jev serves: each usage set and delivered, not off.
fn jev_uses<'a>(usages: impl Iterator<Item = (&'a str, &'a str)>) -> JevUses {
    let mut uses = JevUses::default();
    for (usage, mode) in usages {
        let (Some(usage), Some(mode)) = (DecisionUsage::parse(usage), DecisionMode::parse(mode))
        else {
            continue;
        };
        if mode == DecisionMode::Off || !usage.available() {
            continue;
        }
        match usage {
            DecisionUsage::ModelRouting | DecisionUsage::SkillChoice => uses.requests = true,
            DecisionUsage::MemoryGuard => uses.memories = true,
            DecisionUsage::DocumentLabels => uses.documents = true,
            DecisionUsage::ActionGuard => uses.actions = true,
            DecisionUsage::InjectionDetection | DecisionUsage::MailTriage => {
                uses.external_contents = true;
            }
        }
    }
    uses
}

/// An enabled connector, and the hosts its tools reach as declared.
fn connector(row: &ConnectorInstanceRow) -> ConnectorFact {
    let address = || {
        row.config
            .get("url")
            .and_then(|url| url.as_str())
            .and_then(host)
            .into_iter()
            .collect::<Vec<_>>()
    };
    let (kind, hosts) = match row.connector_type.as_str() {
        "demo_interventions" => (ConnectorKind::Demo, Vec::new()),
        "mcp" if row.config.get("transport").and_then(|t| t.as_str()) == Some("stdio") => {
            (ConnectorKind::McpProcess, Vec::new())
        }
        "mcp" => (ConnectorKind::Mcp, address()),
        "rest" => (ConnectorKind::Rest, address()),
        "microsoft365" => (ConnectorKind::Microsoft365, Vec::new()),
        "google_workspace" => (ConnectorKind::GoogleWorkspace, Vec::new()),
        MAIL_SERVER => (
            ConnectorKind::StandardMail,
            MailServerConfig::read(&row.config)
                .map(|config| config.hosts())
                .unwrap_or_default(),
        ),
        _ => (ConnectorKind::Other, Vec::new()),
    };
    ConnectorFact {
        kind,
        name: row.name.clone(),
        hosts,
    }
}

/// The duration of `kind` the purge applies, as the retention screen reads
/// it.
fn retention(kind: RetentionKind, stored: &[StoredRetention]) -> RetentionFact {
    let duration = RetentionDuration::of(kind, stored);
    RetentionFact {
        kind,
        in_effect: duration.in_effect,
        chosen: duration.duration,
        shortening_at: duration.shortening_applies_at,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;

    fn declared(connector_type: &str, config: Value) -> ConnectorFact {
        connector(&ConnectorInstanceRow {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            connector_type: connector_type.into(),
            name: "Déclaré".into(),
            enabled: true,
            config,
            tool_policies: json!({}),
            status: "ok".into(),
            secret_id: None,
            last_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    #[test]
    fn a_connector_is_read_as_its_declaration_says() {
        let http = declared(
            "mcp",
            json!({ "transport": "http", "url": "https://MCP.example/mcp" }),
        );
        assert_eq!(
            (http.kind, http.hosts),
            (ConnectorKind::Mcp, vec!["mcp.example".into()])
        );
        let process = declared(
            "mcp",
            json!({ "transport": "stdio", "command": "mcp-demo-docs" }),
        );
        assert_eq!(
            (process.kind, process.hosts),
            (ConnectorKind::McpProcess, Vec::new())
        );
        let mail = declared(
            MAIL_SERVER,
            json!({
                "imap": { "host": "imap.example.com", "port": 993 },
                "smtp": { "host": "smtp.example.com", "port": 465, "security": "tls" },
            }),
        );
        assert_eq!(mail.kind, ConnectorKind::StandardMail);
        assert_eq!(mail.hosts, ["imap.example.com", "smtp.example.com"]);
        let unreadable = declared(MAIL_SERVER, json!({ "imap": "?" }));
        assert!(unreadable.hosts.is_empty());
        assert_eq!(
            declared("demo_interventions", json!({})).kind,
            ConnectorKind::Demo
        );
        assert_eq!(declared("inconnu", json!({})).kind, ConnectorKind::Other);
    }

    #[test]
    fn jev_serves_the_usages_set_and_delivered_only() {
        let uses = jev_uses(
            [
                ("model_routing", "observe"),
                ("memory_guard", "off"),
                ("document_labels", "active"),
                ("mail_triage", "active"),
                ("inconnu", "active"),
                ("action_guard", "inconnu"),
            ]
            .into_iter(),
        );
        assert_eq!(
            uses,
            JevUses {
                requests: true,
                documents: true,
                ..JevUses::default()
            }
        );
    }

    #[test]
    fn a_listed_endpoint_is_named_by_its_host() {
        assert_eq!(
            listed_host(true, Some("http://Modeles.Interne:11434/v1")),
            Some("modeles.interne".into())
        );
        assert_eq!(listed_host(false, Some("https://api.example.com")), None);
        assert_eq!(listed_host(true, None), None);
        assert_eq!(host("urn:example:idp"), None);
    }
}
