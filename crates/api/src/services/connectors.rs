//! Instantiates the connectors enabled for a tenant and keeps them alive
//! across requests (connector state, MCP sessions). An instance is rebuilt
//! when its configuration changes (`updated_at`).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use mimorya_agent::tools::ToolOverride;
use mimorya_connectors::{ConnectorRuntime, connector_type};
use mimorya_core::tools::ToolRef;
use mimorya_db::Database;
use mimorya_db::repositories::connectors::{ConnectorInstanceRow, list_connector_instances};
use tokio::sync::Mutex;
use uuid::Uuid;

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
    runtimes: Mutex<HashMap<Uuid, CachedRuntime>>,
}

impl ConnectorService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            runtimes: Mutex::new(HashMap::new()),
        }
    }

    pub async fn tools_for(&self, tenant_id: Uuid) -> anyhow::Result<TenantTools> {
        let mut tx = self.db.begin(tenant_id).await?;
        let instances = list_connector_instances(&mut tx, true).await?;
        tx.commit().await?;

        let mut result = TenantTools::default();
        for instance in instances {
            match self.runtime_for(&instance).await {
                Ok((runtime, overrides)) => {
                    result.tools.extend(runtime.tools.iter().cloned());
                    result.overrides.extend(overrides);
                }
                Err(error) => {
                    tracing::warn!(
                        connector = %instance.id,
                        error = format!("{error:#}"),
                        "Connector unavailable"
                    );
                    result.issues.push(ConnectorIssue {
                        connector: instance.name.clone(),
                        message: "Connecteur indisponible".into(),
                    });
                }
            }
        }
        Ok(result)
    }

    async fn runtime_for(
        &self,
        instance: &ConnectorInstanceRow,
    ) -> anyhow::Result<(Arc<ConnectorRuntime>, HashMap<String, ToolOverride>)> {
        // Invalid overrides make the connector unavailable: ignoring them
        // could silently drop a restriction set by an administrator.
        let overrides: HashMap<String, ToolOverride> =
            serde_json::from_value(instance.tool_policies.clone())?;
        let Some(definition) = connector_type(&instance.connector_type) else {
            anyhow::bail!("unknown connector type {}", instance.connector_type);
        };

        let mut runtimes = self.runtimes.lock().await;
        if let Some(cached) = runtimes.get(&instance.id)
            && cached.version == instance.updated_at
        {
            return Ok((cached.runtime.clone(), overrides));
        }
        if let Some(stale) = runtimes.remove(&instance.id) {
            close_in_background(stale.runtime);
        }
        // Secrets are not wired yet: connectors needing credentials arrive
        // with the encrypted secret store (see docs/ROADMAP.md).
        let runtime = Arc::new(definition.instantiate(&instance.config, None).await?);
        runtimes.insert(
            instance.id,
            CachedRuntime {
                version: instance.updated_at,
                runtime: runtime.clone(),
            },
        );
        Ok((runtime, overrides))
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

fn close_in_background(runtime: Arc<ConnectorRuntime>) {
    tokio::spawn(async move {
        if let Some(closer) = &runtime.closer {
            closer.close().await;
        }
    });
}
