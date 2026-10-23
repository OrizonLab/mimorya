//! Composition root of the API: services, cross-cutting layers, routes and
//! lifecycle. No business rule lives here.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use mimorya_agent::llm::{LlmProvider, PriceTable, create_provider};
use mimorya_connectors::mcp::NetworkPolicy;
use mimorya_db::{Database, PoolOptions};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::config::AppConfig;
use crate::mail::{Mailer, SmtpMailer};
use crate::plugins::csrf::csrf_protection;
use crate::plugins::maintenance::spawn_maintenance;
use crate::plugins::memory_indexing::spawn_memory_indexing;
use crate::plugins::rate_limit::global_rate_limit;
use crate::plugins::request_context::request_context;
use crate::plugins::scheduled_tasks::spawn_scheduled_tasks;
use crate::plugins::security_headers::security_headers;
use crate::plugins::session::resolve_identity;
use crate::plugins::{DistinctLimiter, RateLimiter, RateLimits};
use crate::routes;
use crate::secrets::SecretStore;
use crate::services::background::BackgroundTasks;
use crate::services::connectors::ConnectorSettings;
use crate::services::embeddings::{EmbeddingServiceOptions, default_embedding_factory};
use crate::services::llm::{LlmServiceOptions, default_factory};
use crate::services::runs::{ApprovalNotifier, LessonCapture, RunExecutor};
use crate::services::sso::{SsoService, SystemDns, TxtLookup};
use crate::services::{
    AccessService, ConnectorService, EmbeddingFactory, EmbeddingService, LlmService,
    ProviderFactory, RunEventBus, RunService,
};

const BODY_LIMIT_BYTES: usize = 1_000_000;
const GLOBAL_RATE_LIMIT_PER_MINUTE: u32 = 600;
/// Failed logins allowed per account before it is throttled for the window.
const FAILED_LOGINS_PER_ACCOUNT: u32 = 10;
const FAILED_LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Password reset links an address may ask for in the window.
const RESET_REQUESTS_PER_ADDRESS: u32 = 3;
const RESET_REQUEST_WINDOW: Duration = Duration::from_secs(60 * 60);

pub struct Services {
    pub config: AppConfig,
    pub db: Database,
    pub access: Arc<AccessService>,
    pub connectors: Arc<ConnectorService>,
    pub bus: Arc<RunEventBus>,
    /// The model provider of each company's runs.
    pub llm: Arc<LlmService>,
    /// The embedding model each company's memory is searched with (ADR-074).
    pub embeddings: Arc<EmbeddingService>,
    pub runs: RunService,
    /// Signing in through the companies' identity providers (ADR-063).
    pub sso: Arc<SsoService>,
    /// Sends the invitation and reset e-mails; `None` when no relay is
    /// configured.
    pub mailer: Option<Arc<dyn Mailer>>,
    pub limits: RateLimits,
    /// Work a request starts and does not wait for (reset links, audits of
    /// denied logins): awaited on shutdown.
    pub background: Arc<BackgroundTasks>,
    /// Flips to `true` when the server stops: long-lived streams end.
    pub shutdown: watch::Sender<bool>,
}

pub type AppState = Arc<Services>;

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
}

pub struct App {
    pub router: Router,
    pub services: AppState,
    maintenance: JoinHandle<()>,
    indexing: Option<JoinHandle<()>>,
    scheduling: Option<JoinHandle<()>>,
}

pub async fn build_app(config: AppConfig, options: BuildOptions) -> anyhow::Result<App> {
    let db = Database::connect(
        &config.database_app_url,
        PoolOptions {
            max_connections: 10,
            application_name: "mimorya-api",
        },
    )
    .await?;
    let provider = match options.provider {
        Some(provider) => provider,
        None => config.llm.as_ref().map(create_provider).transpose()?,
    };

    let llm = Arc::new(LlmService::new(
        db.clone(),
        LlmServiceOptions {
            platform: provider,
            secrets: SecretStore::new(config.secrets.clone()),
            listed_endpoints: config.tenant_llm_endpoints.clone(),
            factory: options.provider_factory.unwrap_or_else(default_factory),
            prices: Arc::new(PriceTable::default()),
        },
    ));
    let embeddings = Arc::new(EmbeddingService::new(
        db.clone(),
        EmbeddingServiceOptions {
            secrets: SecretStore::new(config.secrets.clone()),
            listed_endpoints: config.tenant_llm_endpoints.clone(),
            factory: options
                .embedding_factory
                .unwrap_or_else(default_embedding_factory),
            prices: llm.prices().clone(),
        },
    ));
    let access = Arc::new(AccessService::new(db.clone()));
    let connectors = Arc::new(ConnectorService::new(
        db.clone(),
        SecretStore::new(config.secrets.clone()),
        ConnectorSettings {
            network: config.mcp_network,
            start_timeout: config.mcp_start_timeout,
        },
    ));
    let sso = Arc::new(SsoService::new(
        db.clone(),
        config.secrets.clone(),
        options.sso_network.unwrap_or(config.mcp_network),
        &config.web_origin,
        options.dns.unwrap_or_else(SystemDns::shared),
    )?);
    let mailer = match options.mailer {
        Some(mailer) => mailer,
        None => match &config.mail {
            Some(settings) => Some(Arc::new(SmtpMailer::new(settings)?) as Arc<dyn Mailer>),
            None => None,
        },
    };
    let background = Arc::new(BackgroundTasks::default());
    let bus = Arc::new(RunEventBus::default());
    let executor = RunExecutor {
        db: db.clone(),
        access: access.clone(),
        llm: llm.clone(),
        prices: llm.prices().clone(),
        embeddings: embeddings.clone(),
        lessons: LessonCapture::new(db.clone(), llm.clone(), background.clone()),
        connectors: connectors.clone(),
        bus: bus.clone(),
        notifier: ApprovalNotifier::new(
            db.clone(),
            access.clone(),
            mailer.clone(),
            config.web_origin.clone(),
            config.assistant_name.clone(),
            background.clone(),
        ),
        assistant_name: config.assistant_name.clone(),
        llm_judge: config.llm_judge,
    };
    let limits = RateLimits {
        global: RateLimiter::per_minute(
            options
                .global_rate_limit_per_minute
                .unwrap_or(GLOBAL_RATE_LIMIT_PER_MINUTE),
        ),
        login: RateLimiter::per_minute(config.login_rate_limit_per_minute),
        sso_addresses: DistinctLimiter::per_minute(config.login_rate_limit_per_minute),
        sso_sign_in: RateLimiter::per_minute(config.login_rate_limit_per_minute.saturating_mul(2)),
        failed_logins: RateLimiter::new(FAILED_LOGINS_PER_ACCOUNT, FAILED_LOGIN_WINDOW),
        reset_requests: RateLimiter::new(RESET_REQUESTS_PER_ADDRESS, RESET_REQUEST_WINDOW),
    };
    let services = Arc::new(Services {
        runs: RunService::new(db.clone(), access.clone(), executor),
        sso,
        mailer,
        config,
        db: db.clone(),
        access,
        connectors,
        bus: bus.clone(),
        llm,
        embeddings,
        limits,
        background,
        shutdown: watch::channel(false).0,
    });

    // Layers run from the last added to the first: security headers (on
    // every response, refusals included), context, rate limit, CSRF, then
    // session resolution.
    let router = routes::router(&services)
        .layer(from_fn_with_state(services.clone(), resolve_identity))
        .layer(from_fn_with_state(services.clone(), csrf_protection))
        .layer(from_fn_with_state(services.clone(), global_rate_limit))
        .layer(from_fn_with_state(services.clone(), request_context))
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .layer(from_fn(security_headers))
        .with_state(services.clone());

    Ok(App {
        router,
        maintenance: spawn_maintenance(services.clone()),
        indexing: (!options.manual_indexing).then(|| spawn_memory_indexing(services.clone())),
        scheduling: (!options.manual_scheduling).then(|| spawn_scheduled_tasks(services.clone())),
        services,
    })
}

impl App {
    /// Takes up again the runs a stop of the service left running (ADR-053),
    /// once managers are told of the requests whose notice it lost
    /// (ADR-070); how many runs there were. Once, at startup, once listening,
    /// before serving; it fails while another process holds the run
    /// executor's lock.
    pub async fn recover_interrupted_runs(&self) -> anyhow::Result<usize> {
        self.services.runs.recover_interrupted().await
    }

    /// Ends event streams so that open connections can close.
    pub fn begin_shutdown(&self) {
        self.services.shutdown.send_replace(true);
    }

    /// Waits for background runs and requests' background work, then
    /// releases connectors and connections.
    pub async fn shutdown(self) {
        self.begin_shutdown();
        // Ended before the runs are awaited: its expiry sweep may start one.
        self.maintenance.abort();
        let _ = self.maintenance.await;
        // So is this one, which starts runs: it stops between two tasks,
        // never in the middle of firing one.
        if let Some(scheduling) = self.scheduling {
            let _ = scheduling.await;
        }
        if let Some(indexing) = self.indexing {
            indexing.abort();
            let _ = indexing.await;
        }
        self.services.runs.idle().await;
        self.services.background.idle().await;
        self.services.connectors.close().await;
        self.services.db.close().await;
    }
}
