//! How the register reads the company's settings: its models and the
//! server each is reached at, the usages of its decision service, its
//! connectors and the hosts they reach, its durations. Only a host is ever
//! taken from an address: never its credentials, path or parameters.

use mimorya_agent::decision::{DecisionMode, DecisionUsage};
use mimorya_agent::llm::embedding::embedding_uses_listed_endpoint;
use mimorya_agent::llm::{default_endpoint, official_endpoint, uses_listed_endpoint};
use mimorya_connectors::standard_mail::{MAIL_SERVER, MailServerConfig};
use mimorya_core::account_links::{GOOGLE_WORKSPACE, MICROSOFT365};
use mimorya_core::processing_register::{
    ChosenBy, ConnectorFact, ConnectorKind, DecisionUses, ModelService, RetentionFact,
};
use mimorya_core::retention::RetentionKind;
use mimorya_db::repositories::connectors::ConnectorInstanceRow;
use mimorya_db::repositories::embedding_settings::EmbeddingSettingsRow;
use mimorya_db::repositories::llm_settings::LlmSettingsRow;
use mimorya_db::repositories::retention_durations::StoredRetention;
use url::Url;

use crate::services::admin::retention::RetentionDuration;

/// A model the company chose, as stored: at the endpoint the operator
/// lists, for a kind served there (a hosted kind has none, ADR-024).
pub(super) fn chosen_model(row: &LlmSettingsRow) -> ModelService {
    ModelService {
        provider: row.provider.clone(),
        server_host: listed_host(uses_listed_endpoint(&row.provider), row.base_url.as_deref()),
        chosen_by: ChosenBy::Company,
    }
}

/// The company's embedding model, as stored.
pub(super) fn embedding_model(row: &EmbeddingSettingsRow) -> ModelService {
    ModelService {
        provider: row.provider.clone(),
        server_host: listed_host(
            embedding_uses_listed_endpoint(&row.provider),
            row.base_url.as_deref(),
        ),
        chosen_by: ChosenBy::Company,
    }
}

/// The operator's model (`LLM_PROVIDER`): at the address it set
/// (`LLM_BASE_URL`, `ANTHROPIC_BASE_URL`), or the kind's own, named by its
/// host unless it is the provider's official one.
pub(super) fn platform_model(kind: &str, base_url: Option<&str>) -> ModelService {
    let official = official_endpoint(kind);
    let reached = base_url.or(default_endpoint(kind)).or(official);
    let at_provider = matches!(
        (reached, official),
        (Some(reached), Some(official)) if same_address(reached, official)
    );
    ModelService {
        provider: kind.to_string(),
        server_host: if at_provider {
            None
        } else {
            reached.and_then(host)
        },
        chosen_by: ChosenBy::Operator,
    }
}

fn same_address(one: &str, other: &str) -> bool {
    one.trim_end_matches('/') == other.trim_end_matches('/')
}

/// The host of an endpoint the operator lists, for a kind served there.
pub(super) fn listed_host(listed: bool, base_url: Option<&str>) -> Option<String> {
    listed.then(|| base_url.and_then(host)).flatten()
}

/// The host of an address, if it is one.
pub(super) fn host(address: &str) -> Option<String> {
    Url::parse(address)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
}

/// What the decision service is asked: each usage set and delivered, not
/// off.
pub(super) fn decision_uses<'a>(usages: impl Iterator<Item = (&'a str, &'a str)>) -> DecisionUses {
    let mut uses = DecisionUses::default();
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

/// A connector, enabled or not, and the hosts its tools reach as declared.
pub(super) fn connector(row: &ConnectorInstanceRow) -> ConnectorFact {
    let address = || {
        row.config
            .get("url")
            .and_then(|url| url.as_str())
            .and_then(host)
            .into_iter()
            .collect::<Vec<_>>()
    };
    let process = row.config.get("transport").and_then(|t| t.as_str()) == Some("stdio");
    let (kind, hosts) = match row.connector_type.as_str() {
        "demo_interventions" => (ConnectorKind::Demo, Vec::new()),
        "mcp" if process => (ConnectorKind::McpProcess, Vec::new()),
        "mcp" => (ConnectorKind::Mcp, address()),
        "rest" => (ConnectorKind::Rest, address()),
        MICROSOFT365 => (ConnectorKind::Microsoft365, Vec::new()),
        GOOGLE_WORKSPACE => (ConnectorKind::GoogleWorkspace, Vec::new()),
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
        enabled: row.enabled,
    }
}

/// The duration of `kind` the purge applies, as the retention screen reads
/// it.
pub(super) fn retention(kind: RetentionKind, stored: &[StoredRetention]) -> RetentionFact {
    let duration = RetentionDuration::of(kind, stored);
    RetentionFact {
        kind,
        in_effect: duration.in_effect,
        chosen: duration.duration,
        shortening_at: duration.shortening_applies_at,
    }
}
