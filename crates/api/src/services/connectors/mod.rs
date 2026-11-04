//! Instantiates the connectors enabled for a tenant and keeps them alive
//! across requests (connector state, MCP sessions). An instance is rebuilt
//! when its configuration changes (`updated_at`) and released when it is
//! changed, disabled or deleted.
//!
//! Instances start side by side; a start in progress is shared by the runs
//! that need it, and a failed start is remembered for a while (`cache`,
//! ADR-049): a server that does not answer delays the runs of one start,
//! then one run per retry delay, by the start timeout at most. One connector failing (server, token,
//! policies, a tool name taken) makes that connector alone unavailable. How
//! each start went is recorded on the instance (`status`, `last_error`).
//! The starts an administration asks for are bounded per company
//! (`admin_starts`, ADR-055). The tools of the connectors that reach
//! outside the platform can be withheld (`tenant_tools`, ADR-153). An MCP
//! server declared with OAuth starts with its authorization's access token,
//! its session retired before the token expires (`credential`, ADR-171).

mod admin_starts;
mod cache;
mod credential;
mod failures;
mod policies;
mod reach;
mod tenant_tools;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures::future::join_all;
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::{ConnectorRuntime, InstanceContext, connector_type};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::connectors::{
    ConnectorInstanceRow, list_connector_instances, report_connector_status,
};
use tokio::time::Instant;
use uuid::Uuid;

use admin_starts::AdminStarts;
pub use admin_starts::{AdminStart, MAX_ADMIN_STARTS, TOO_MANY_ADMIN_STARTS};
pub use cache::RETRY_AFTER;
use cache::{Origin, RuntimeCache, Unavailable};
pub use credential::{Credential, credential_of};
pub use failures::{failure, opened};
pub use mimorya_connectors::outbound::CONNECTOR_USE;
pub use policies::{Policies, read_policies, scoped_overrides};
pub use reach::unavailable_in_reach;
pub use tenant_tools::TenantTools;

use super::invalidations::{Invalidation, Invalidations};
use crate::secrets::SecretStore;

/// The secret holding an instance's access token.
pub fn token_secret_name(instance_id: Uuid) -> String {
    format!("connector.{instance_id}.token")
}

/// The secret of the OAuth client declared on an MCP server (ADR-171).
pub fn client_secret_name(instance_id: Uuid) -> String {
    format!("connector.{instance_id}.client_secret")
}

/// What the access token of an MCP server's OAuth authorization is sealed
/// as: never another connector's (ADR-171).
pub fn access_sealed_as(instance_id: Uuid) -> String {
    format!("connector.{instance_id}.oauth_access")
}

/// What its refresh token is sealed as.
pub fn refresh_sealed_as(instance_id: Uuid) -> String {
    format!("connector.{instance_id}.oauth_refresh")
}

/// What the operator lets connectors do.
#[derive(Debug, Clone, Copy)]
pub struct ConnectorSettings {
    pub network: NetworkPolicy,
    pub start_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorIssue {
    /// The instance's name.
    pub connector: String,
    pub connector_type: String,
    pub message: String,
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
    admin_starts: AdminStarts,
    invalidations: Invalidations,
}

impl ConnectorService {
    pub fn new(
        db: Database,
        secrets: SecretStore,
        settings: ConnectorSettings,
        invalidations: Invalidations,
    ) -> Self {
        Self {
            db,
            secrets,
            settings,
            cache: RuntimeCache::default(),
            admin_starts: AdminStarts::default(),
            invalidations,
        }
    }

    pub fn network(&self) -> NetworkPolicy {
        self.settings.network
    }

    /// A slot for a start an administration of `tenant_id` asks for, held
    /// until it is dropped; refused while the company has
    /// `MAX_ADMIN_STARTS` under way.
    pub fn claim_admin_start(&self, tenant_id: Uuid) -> Result<AdminStart, DomainError> {
        self.admin_starts.claim(tenant_id)
    }

    pub async fn tools_for(&self, tenant_id: Uuid) -> anyhow::Result<TenantTools> {
        let ticket = self.cache.ticket().await;
        // Instances and tokens read in one snapshot: a token always goes
        // with the address it was saved, or authorized, for.
        let mut tx = self.db.begin_snapshot(tenant_id).await?;
        let mut instances = list_connector_instances(&mut tx, true).await?;
        // The oldest keeps its tools when two connectors name the same one.
        instances.sort_by_key(|instance| instance.created_at);
        let mut credentials = Vec::with_capacity(instances.len());
        for instance in &instances {
            credentials.push(credential_of(&mut tx, &self.secrets, instance).await?);
        }
        tx.commit().await?;

        let prepared = join_all(
            instances
                .iter()
                .zip(credentials)
                .map(|(instance, credential)| self.prepare(instance, credential, ticket)),
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
                    result.give(
                        &instance.name,
                        &instance.connector_type,
                        &ready.runtime.tools,
                        ready.overrides,
                    );
                    None
                }
                Err(unavailable) => {
                    result.issues.push(ConnectorIssue {
                        connector: instance.name.clone(),
                        connector_type: instance.connector_type.clone(),
                        message: "Connecteur indisponible".into(),
                    });
                    match unavailable.origin {
                        Origin::Changed => continue,
                        Origin::Remembered => {}
                        Origin::Fresh => {
                            tracing::warn!(connector = %instance.id, error = %unavailable.message, "Connector unavailable");
                        }
                    }
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
        credential: anyhow::Result<Credential>,
        ticket: u64,
    ) -> Result<Ready, Unavailable> {
        let policies = read_policies(instance).map_err(Unavailable::new)?;
        let credential = credential.map_err(|error| Unavailable::new(error.to_string()))?;
        let runtime = self
            .cache
            .get_or_start(instance.id, instance.updated_at, ticket, || async {
                self.start(&instance.connector_type, &instance.config, &credential)
                    .await
                    .map_err(|error| failure(&error, credential.token.as_deref()))
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
        credential: &Credential,
    ) -> anyhow::Result<ConnectorRuntime> {
        let Some(definition) = connector_type(connector) else {
            anyhow::bail!("Type de connecteur inconnu : {connector}");
        };
        // Past, now; too far for the clock, as if it did not expire.
        let until = credential.expires_at.and_then(|at| {
            let left = (at - Utc::now()).to_std().unwrap_or_default();
            Instant::now().checked_add(left)
        });
        let context = InstanceContext {
            secret: credential.token.as_deref(),
            secret_until: until,
            network: self.settings.network,
            start_timeout: self.settings.start_timeout,
            ..InstanceContext::default()
        };
        definition.instantiate(config, context).await
    }

    /// Releases the live instance after a change, a deactivation or a
    /// deletion, here and in the other processes.
    pub async fn invalidate(&self, instance_id: Uuid) {
        self.forget(instance_id).await;
        self.invalidations
            .announce(Invalidation::Connector(instance_id));
    }

    /// Releases the live instance here: another process changed it.
    pub(crate) async fn forget(&self, instance_id: Uuid) {
        self.cache.invalidate(instance_id).await;
    }

    /// The instance answered a test: the next run tries it again at once,
    /// whatever process runs it.
    pub async fn retry_now(&self, instance_id: Uuid) {
        self.retry_here(instance_id).await;
        self.invalidations
            .announce(Invalidation::ConnectorRetry(instance_id));
    }

    pub(crate) async fn retry_here(&self, instance_id: Uuid) {
        self.cache.retry_now(instance_id).await;
    }

    /// Releases every live connector (shutdown).
    pub async fn close(&self) {
        self.cache.close().await;
    }
}
