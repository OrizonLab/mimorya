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
//! its session retired before the token expires (`credential`, ADR-171),
//! the token renewed before the instances are read (`renewal`); the tokens
//! the platform lets go of are revoked at its server (`revocation`,
//! ADR-172). On a server each person authorizes, a run is given the
//! instance of the person it serves, started with their own authorization,
//! never another's (`served`, ADR-174).

mod admin_starts;
mod cache;
mod credential;
mod failures;
mod grants;
mod policies;
mod reach;
mod renewal;
mod revocation;
mod served;
mod tenant_tools;

use std::time::Duration;

use chrono::Utc;
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::{ConnectorRuntime, InstanceContext, connector_type};
use mimorya_core::DomainError;
use mimorya_db::Database;
use tokio::time::Instant;
use uuid::Uuid;

use admin_starts::AdminStarts;
pub use admin_starts::{AdminStart, MAX_ADMIN_STARTS, TOO_MANY_ADMIN_STARTS};
use cache::RuntimeCache;
pub use cache::{MAX_PERSON_SESSIONS, PERSON_IDLE, RETRY_AFTER};
pub use credential::{
    Credential, CredentialFor, UNOPENED_PERSON, credential_for, credential_of, each_person,
    given_under,
};
pub use failures::{failure, opened};
pub use grants::{
    AUTHORIZATION_REMOVED, PERSON_REMOVED, SealedGrant, seal_grant, seal_person_grant,
};
pub use mimorya_connectors::outbound::CONNECTOR_USE;
pub use policies::{Policies, read_policies, scoped_overrides};
pub use reach::{to_authorize_in_reach, unavailable_in_reach};
pub(crate) use renewal::Whose;
pub use revocation::{LetGo, REVOCATION_BOUND, Unkept, revoke_within};
pub use served::{NOT_FOR_THEM, Served, TO_AUTHORIZE, WITHHELD_PERSONAL};
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

/// What the access token a person's own authorization of an MCP server
/// brought is sealed as (ADR-174): opened for that person only.
pub fn person_access_sealed_as(instance_id: Uuid, user_id: Uuid) -> String {
    format!("connector.{instance_id}.person.{user_id}.oauth_access")
}

/// What its refresh token is sealed as.
pub fn person_refresh_sealed_as(instance_id: Uuid, user_id: Uuid) -> String {
    format!("connector.{instance_id}.person.{user_id}.oauth_refresh")
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
    /// Whether the person the run serves must first authorize it for
    /// themself (ADR-174).
    pub to_authorize: bool,
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

    /// Releases a person's instance of a server here, their authorization
    /// gone (ADR-174). Another process never gives it to a run (it reads the
    /// authorization first) and releases it once idle.
    pub async fn forget_person(&self, instance_id: Uuid, user_id: Uuid) {
        self.cache.forget_person(instance_id, user_id).await;
    }

    /// Releases the people's instances no run was given for a while.
    pub async fn release_idle(&self) {
        self.cache.release_idle().await;
    }

    /// How many people's instances this process keeps live (ADR-174).
    pub async fn person_sessions(&self) -> usize {
        self.cache.person_sessions().await
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
