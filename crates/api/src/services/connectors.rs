//! Instantiates the connectors enabled for a tenant and keeps them alive
//! across requests (connector state, MCP sessions). An instance is rebuilt
//! when its configuration changes (`updated_at`) and released when it is
//! changed, disabled or deleted. Starting one never blocks the others.
//!
//! An instance's tool policies only apply to its own tools, and an MCP tool
//! always requires `connector:use` (ADR-025). How each start went is recorded
//! on the instance (`status`, `last_error`) for its administrators.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use mimorya_agent::tools::ToolOverride;
use mimorya_connectors::mcp::NetworkPolicy;
use mimorya_connectors::{ConnectorRuntime, InstanceContext, connector_type};
use mimorya_core::tools::ToolRef;
use mimorya_db::Database;
use mimorya_db::repositories::connectors::{
    ConnectorInstanceRow, list_connector_instances, report_connector_status,
};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::secrets::{SecretRead, SecretStore};

/// Every MCP tool requires it, whatever its policy says.
pub const CONNECTOR_USE: &str = "connector:use";
const MAX_ERROR_CHARS: usize = 300;

/// The secret holding an instance's access token.
pub fn token_secret_name(instance_id: Uuid) -> String {
    format!("connector.{instance_id}.token")
}

struct CachedRuntime {
    version: DateTime<Utc>,
    runtime: Arc<ConnectorRuntime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorIssue {
    pub connector: String,
    pub message: String,
}

#[derive(Default)]
pub struct TenantTools {
    pub tools: Vec<ToolRef>,
    pub overrides: HashMap<String, ToolOverride>,
    pub issues: Vec<ConnectorIssue>,
}

pub struct ConnectorService {
    db: Database,
    secrets: SecretStore,
    network: NetworkPolicy,
    runtimes: Mutex<HashMap<Uuid, CachedRuntime>>,
}

impl ConnectorService {
    pub fn new(db: Database, secrets: SecretStore, network: NetworkPolicy) -> Self {
        Self {
            db,
            secrets,
            network,
            runtimes: Mutex::new(HashMap::new()),
        }
    }

    pub fn network(&self) -> NetworkPolicy {
        self.network
    }

    pub async fn tools_for(&self, tenant_id: Uuid) -> anyhow::Result<TenantTools> {
        let mut tx = self.db.begin(tenant_id).await?;
        let mut instances = list_connector_instances(&mut tx, true).await?;
        // The oldest keeps its tools when two connectors name the same one.
        instances.sort_by_key(|instance| instance.created_at);
        let mut secrets = HashMap::new();
        for instance in &instances {
            let read = self
                .secrets
                .read(&mut tx, tenant_id, &token_secret_name(instance.id))
                .await?;
            secrets.insert(instance.id, opened(read));
        }
        tx.commit().await?;

        let mut result = TenantTools::default();
        let mut names = HashSet::new();
        let mut reports = Vec::new();
        for instance in instances {
            let opened = secrets
                .remove(&instance.id)
                .unwrap_or_else(|| Err(anyhow::anyhow!("secret not loaded")));
            let secret = opened.as_ref().ok().cloned().flatten();
            let started = match opened {
                Ok(secret) => self.runtime_for(&instance, secret.as_deref()).await,
                Err(error) => Err(error),
            };
            let outcome = started.and_then(|runtime| {
                let tools: Vec<String> = runtime
                    .tools
                    .iter()
                    .map(|t| t.info().name.clone())
                    .collect();
                if tools.iter().any(|name| names.contains(name)) {
                    anyhow::bail!("Outils portant le même nom qu’un autre connecteur");
                }
                Ok((runtime, tools))
            });
            match outcome {
                Ok((runtime, tools)) => {
                    result
                        .overrides
                        .extend(scoped_overrides(&instance, &tools)?);
                    names.extend(tools);
                    result.tools.extend(runtime.tools.iter().cloned());
                    if instance.status == "error" {
                        reports.push((instance.id, None));
                    }
                }
                Err(error) => {
                    tracing::warn!(connector = %instance.id, error = format!("{error:#}"), "Connector unavailable");
                    reports.push((instance.id, Some(failure(&error, secret.as_deref()))));
                    result.issues.push(ConnectorIssue {
                        connector: instance.name.clone(),
                        message: "Connecteur indisponible".into(),
                    });
                }
            }
        }
        if !reports.is_empty() {
            let mut tx = self.db.begin(tenant_id).await?;
            for (id, error) in &reports {
                report_connector_status(&mut tx, *id, error.as_deref()).await?;
            }
            tx.commit().await?;
        }
        Ok(result)
    }

    async fn runtime_for(
        &self,
        instance: &ConnectorInstanceRow,
        secret: Option<&str>,
    ) -> anyhow::Result<Arc<ConnectorRuntime>> {
        {
            let mut runtimes = self.runtimes.lock().await;
            if let Some(cached) = runtimes.get(&instance.id)
                && cached.version == instance.updated_at
            {
                return Ok(cached.runtime.clone());
            }
            if let Some(stale) = runtimes.remove(&instance.id) {
                close_in_background(stale.runtime);
            }
        }
        // Started without holding the lock: a slow server delays its own
        // instance only.
        let runtime = Arc::new(
            self.start(&instance.connector_type, &instance.config, secret)
                .await?,
        );
        let mut runtimes = self.runtimes.lock().await;
        if let Some(cached) = runtimes.get(&instance.id)
            && cached.version >= instance.updated_at
        {
            close_in_background(runtime);
            return Ok(cached.runtime.clone());
        }
        if let Some(stale) = runtimes.insert(
            instance.id,
            CachedRuntime {
                version: instance.updated_at,
                runtime: runtime.clone(),
            },
        ) {
            close_in_background(stale.runtime);
        }
        Ok(runtime)
    }

    /// A fresh instance (not cached): the caller releases it.
    pub async fn start(
        &self,
        connector: &str,
        config: &serde_json::Value,
        secret: Option<&str>,
    ) -> anyhow::Result<ConnectorRuntime> {
        let Some(definition) = connector_type(connector) else {
            anyhow::bail!("unknown connector type {connector}");
        };
        let context = InstanceContext {
            secret,
            network: self.network,
        };
        definition.instantiate(config, context).await
    }

    /// Releases the live instance after a change, a deactivation or a deletion.
    pub async fn invalidate(&self, instance_id: Uuid) {
        if let Some(stale) = self.runtimes.lock().await.remove(&instance_id) {
            close_in_background(stale.runtime);
        }
    }

    /// Releases every live connector (shutdown).
    pub async fn close(&self) {
        let runtimes: Vec<CachedRuntime> =
            self.runtimes.lock().await.drain().map(|(_, c)| c).collect();
        for cached in runtimes {
            if let Some(closer) = &cached.runtime.closer {
                closer.close().await;
            }
        }
    }
}

/// The instance's policies for its own tools only; an MCP tool keeps
/// requiring `connector:use`. Unreadable policies make the connector
/// unavailable: ignoring them could drop a restriction an administrator set.
fn scoped_overrides(
    instance: &ConnectorInstanceRow,
    tools: &[String],
) -> anyhow::Result<HashMap<String, ToolOverride>> {
    let policies: HashMap<String, ToolOverride> =
        serde_json::from_value(instance.tool_policies.clone())?;
    Ok(policies
        .into_iter()
        .filter(|(name, _)| tools.contains(name))
        .map(|(name, mut policy)| {
            if instance.connector_type == "mcp"
                && let Some(permissions) = policy.permissions.as_mut()
                && !permissions.iter().any(|p| p == CONNECTOR_USE)
            {
                permissions.push(CONNECTOR_USE.into());
            }
            (name, policy)
        })
        .collect())
}

/// The token of an instance, or why it cannot be used.
pub fn opened(read: SecretRead) -> anyhow::Result<Option<String>> {
    match read {
        SecretRead::Absent => Ok(None),
        SecretRead::Value(token) => Ok(Some(token)),
        SecretRead::NoMasterKey => {
            anyhow::bail!("Ce serveur n’a pas de clé maîtresse : le jeton ne peut pas être lu")
        }
        SecretRead::Unreadable => {
            anyhow::bail!(
                "Le jeton enregistré ne peut pas être déchiffré : saisissez-le de nouveau"
            )
        }
    }
}

/// What administrators see of a failed start: what happened, in French,
/// then a short technical detail, without the token even if a server echoed it.
pub fn failure(error: &anyhow::Error, secret: Option<&str>) -> String {
    let mut detail = format!("{error:#}");
    if let Some(secret) = secret.filter(|s| !s.is_empty()) {
        detail = detail.replace(secret, "***");
    }
    let lowered = detail.to_lowercase();
    let summary = if lowered.contains("aucune adresse publique") || lowered.contains("privé") {
        "Adresse refusée : elle ne mène pas à un serveur public"
    } else if lowered.contains("401") || lowered.contains("403") || lowered.contains("unauthorized")
    {
        "Accès refusé par le serveur : jeton manquant ou invalide"
    } else if lowered.contains("in time") || lowered.contains("timed out") {
        "Le serveur n’a pas répondu à temps"
    } else if lowered.contains("sending request") || lowered.contains("connect") {
        "Serveur injoignable à cette adresse"
    } else if lowered.contains("stdio") {
        "Les serveurs lancés en local (stdio) ne sont pas autorisés sur ce serveur"
    } else {
        "Le serveur n’a pas répondu comme un serveur MCP"
    };
    format!("{summary} (détail : {detail})")
        .chars()
        .take(MAX_ERROR_CHARS)
        .collect()
}

fn close_in_background(runtime: Arc<ConnectorRuntime>) {
    tokio::spawn(async move {
        if let Some(closer) = &runtime.closer {
            closer.close().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_failure_never_shows_the_token() {
        let error = anyhow::anyhow!("401 Unauthorized: token jeton-secret-123 refused");
        let shown = failure(&error, Some("jeton-secret-123"));
        assert!(!shown.contains("jeton-secret-123"));
        assert!(shown.contains("***"));
        assert!(shown.starts_with("Accès refusé par le serveur"));
        let long = anyhow::anyhow!("{}", "x".repeat(1000));
        assert_eq!(failure(&long, None).chars().count(), MAX_ERROR_CHARS);
        let down = anyhow::anyhow!("error sending request for url (http://127.0.0.1:9/mcp)");
        assert!(failure(&down, None).starts_with("Serveur injoignable"));
    }
}
