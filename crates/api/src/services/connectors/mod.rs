//! Instantiates the connectors enabled for a tenant and keeps them alive
//! across requests (connector state, MCP sessions). An instance is rebuilt
//! when its configuration changes (`updated_at`) and released when it is
//! changed, disabled or deleted.
//!
//! Instances start side by side, and a failed start is remembered for a
//! while (`cache`): a server that does not answer delays one run per retry
//! delay, by the start timeout at most. One connector failing (server, token,
//! policies, a tool name taken) makes that connector alone unavailable. How
//! each start went is recorded on the instance (`status`, `last_error`).

mod cache;
mod failures;
mod policies;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use mimorya_agent::tools::ToolOverride;
use mimorya_connectors::mcp::NetworkPolicy;
use mimorya_connectors::{ConnectorRuntime, InstanceContext, connector_type};
use mimorya_core::tools::ToolRef;
use mimorya_db::Database;
use mimorya_db::repositories::connectors::{
    ConnectorInstanceRow, list_connector_instances, report_connector_status,
};
use uuid::Uuid;

pub use cache::RETRY_AFTER;
use cache::{RuntimeCache, Unavailable};
pub use failures::{failure, opened};
pub use mimorya_connectors::mcp::CONNECTOR_USE;
pub use policies::{Policies, read_policies, scoped_overrides};

use crate::secrets::SecretStore;

/// The secret holding an instance's access token.
pub fn token_secret_name(instance_id: Uuid) -> String {
    format!("connector.{instance_id}.token")
}

/// What the operator lets connectors do.
#[derive(Debug, Clone, Copy)]
pub struct ConnectorSettings {
    pub network: NetworkPolicy,
    pub start_timeout: Duration,
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

struct Ready {
    runtime: Arc<ConnectorRuntime>,
    names: Vec<String>,
    overrides: Policies,
}

pub struct ConnectorService {
    db: Database,
    secrets: SecretStore,
    settings: ConnectorSettings,
    cache: RuntimeCache,
}

impl ConnectorService {
    pub fn new(db: Database, secrets: SecretStore, settings: ConnectorSettings) -> Self {
        Self {
            db,
            secrets,
            settings,
            cache: RuntimeCache::default(),
        }
    }

    pub fn network(&self) -> NetworkPolicy {
        self.settings.network
    }

    pub async fn tools_for(&self, tenant_id: Uuid) -> anyhow::Result<TenantTools> {
        let ticket = self.cache.ticket().await;
        // Instances and tokens read in one snapshot: a token always goes
        // with the address it was saved with.
        let mut tx = self.db.begin_snapshot(tenant_id).await?;
        let mut instances = list_connector_instances(&mut tx, true).await?;
        // The oldest keeps its tools when two connectors name the same one.
        instances.sort_by_key(|instance| instance.created_at);
        let mut secrets = Vec::with_capacity(instances.len());
        for instance in &instances {
            let read = self
                .secrets
                .read(&mut tx, tenant_id, &token_secret_name(instance.id))
                .await?;
            secrets.push(opened(read));
        }
        tx.commit().await?;

        let prepared = join_all(
            instances
                .iter()
                .zip(secrets)
                .map(|(instance, secret)| self.prepare(instance, secret, ticket)),
        )
        .await;
        let mut result = TenantTools::default();
        let mut names = HashSet::new();
        let mut reports = Vec::new();
        for (instance, prepared) in instances.iter().zip(prepared) {
            let outcome = prepared.and_then(|ready| {
                if ready.names.iter().any(|name| names.contains(name)) {
                    return Err(Unavailable::new(
                        "Des outils portent le même nom que ceux d’un connecteur plus ancien",
                    ));
                }
                Ok(ready)
            });
            let error = match outcome {
                Ok(ready) => {
                    names.extend(ready.names);
                    result.tools.extend(ready.runtime.tools.iter().cloned());
                    result.overrides.extend(ready.overrides);
                    None
                }
                Err(unavailable) => {
                    if !unavailable.remembered {
                        tracing::warn!(connector = %instance.id, error = %unavailable.message, "Connector unavailable");
                    }
                    result.issues.push(ConnectorIssue {
                        connector: instance.name.clone(),
                        message: "Connecteur indisponible".into(),
                    });
                    Some(unavailable.message)
                }
            };
            let status = if error.is_some() { "error" } else { "ok" };
            if instance.status != status || instance.last_error != error {
                reports.push((instance.id, instance.updated_at, error));
            }
        }
        if !reports.is_empty() {
            let mut tx = self.db.begin(tenant_id).await?;
            for (id, version, error) in &reports {
                report_connector_status(&mut tx, *id, *version, error.as_deref()).await?;
            }
            tx.commit().await?;
        }
        Ok(result)
    }

    /// The instance's tools and policies, or why it is unavailable.
    async fn prepare(
        &self,
        instance: &ConnectorInstanceRow,
        secret: anyhow::Result<Option<String>>,
        ticket: u64,
    ) -> Result<Ready, Unavailable> {
        let policies = read_policies(instance).map_err(Unavailable::new)?;
        let secret = secret.map_err(|error| Unavailable::new(error.to_string()))?;
        let runtime = self
            .cache
            .get_or_start(instance.id, instance.updated_at, ticket, || async {
                self.start(
                    &instance.connector_type,
                    &instance.config,
                    secret.as_deref(),
                )
                .await
                .map_err(|error| failure(&error, secret.as_deref()))
            })
            .await?;
        let names: Vec<String> = runtime
            .tools
            .iter()
            .map(|tool| tool.info().name.clone())
            .collect();
        let overrides = scoped_overrides(instance, policies, &names);
        Ok(Ready {
            runtime,
            names,
            overrides,
        })
    }

    /// A fresh instance (not cached): the caller releases it.
    pub async fn start(
        &self,
        connector: &str,
        config: &serde_json::Value,
        secret: Option<&str>,
    ) -> anyhow::Result<ConnectorRuntime> {
        let Some(definition) = connector_type(connector) else {
            anyhow::bail!("Type de connecteur inconnu : {connector}");
        };
        let context = InstanceContext {
            secret,
            network: self.settings.network,
            start_timeout: self.settings.start_timeout,
        };
        definition.instantiate(config, context).await
    }

    /// Releases the live instance after a change, a deactivation or a deletion.
    pub async fn invalidate(&self, instance_id: Uuid) {
        self.cache.invalidate(instance_id).await;
    }

    /// The instance answered a test: the next run tries it again at once.
    pub async fn retry_now(&self, instance_id: Uuid) {
        self.cache.retry_now(instance_id).await;
    }

    /// Releases every live connector (shutdown).
    pub async fn close(&self) {
        self.cache.close().await;
    }
}
