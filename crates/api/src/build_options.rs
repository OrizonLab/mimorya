//! What a test changes of the server it builds (`build_app`): the model,
//! the mailer, the networks and services it reaches. A production start
//! leaves every option to its default.

use std::sync::Arc;

use mimorya_agent::llm::LlmProvider;
use mimorya_connectors::outbound::NetworkPolicy;

use crate::mail::Mailer;
use crate::services::runs::LeaseTiming;
use crate::services::sso::TxtLookup;
use crate::services::{EmbeddingFactory, ProviderFactory};

#[derive(Default)]
pub struct BuildOptions {
    /// Replaces the provider built from configuration (tests); `Some(None)`
    /// runs the server without any provider.
    pub provider: Option<Option<Arc<dyn LlmProvider>>>,
    /// Replaces how a company's own provider is built (tests).
    pub provider_factory: Option<ProviderFactory>,
    /// Replaces how a company's embedding model is built (tests).
    pub embedding_factory: Option<EmbeddingFactory>,
    /// Leaves memories to be indexed on demand (`index_pending`), not in the
    /// background (tests that count what a pass embeds).
    pub manual_indexing: bool,
    /// Replaces how long a run's lease lasts and how often leases are renewed
    /// and taken up (tests).
    pub run_leases: Option<LeaseTiming>,
    /// Leaves scheduled tasks to be fired on demand (`fire_due_tasks`), not
    /// every minute once interrupted runs are taken up (tests that set when
    /// one is due).
    pub manual_scheduling: bool,
    /// Replaces the mailer built from configuration (tests); `Some(None)`
    /// sends nothing.
    pub mailer: Option<Option<Arc<dyn Mailer>>>,
    /// Replaces the network the identity providers are reached on, the
    /// connectors' otherwise (end-to-end tests: a local provider, while
    /// connectors keep to the public network).
    pub sso_network: Option<NetworkPolicy>,
    /// Replaces the limit of requests per minute and address (end-to-end
    /// tests: every request comes through the one web relay).
    pub global_rate_limit_per_minute: Option<u32>,
    /// Replaces the system's DNS, where a domain's proof is read (tests).
    pub dns: Option<Arc<dyn TxtLookup>>,
    /// Replaces the issuer a company's Microsoft 365 application sends its
    /// people to, its directory's at Microsoft, as stored (end-to-end
    /// tests: the local provider, ADR-118).
    pub microsoft365_issuer: Option<String>,
}
