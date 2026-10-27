//! Composition root of the API: services, cross-cutting layers, routes and
//! lifecycle. No business rule lives here.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use mimorya_agent::llm::{PriceTable, create_provider};
use mimorya_db::{Database, PoolOptions};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub use crate::build_options::BuildOptions;
use crate::config::AppConfig;
use crate::mail::{Mailer, SmtpMailer};
use crate::plugins::csrf::csrf_protection;
use crate::plugins::maintenance::spawn_maintenance;
use crate::plugins::memory_indexing::spawn_memory_indexing;
use crate::plugins::rate_limit::global_rate_limit;
use crate::plugins::request_context::request_context;
use crate::plugins::run_event_relay::{spawn_delta_outbox, spawn_run_event_relay};
use crate::plugins::run_leases::{spawn_lease_renewal, spawn_lease_takeover};
use crate::plugins::run_queue::spawn_run_queue;
use crate::plugins::scheduled_tasks::spawn_scheduled_tasks;
use crate::plugins::security_headers::security_headers;
use crate::plugins::session::resolve_identity;
use crate::plugins::{RateLimiter, RateLimits, SharedDistinctLimiter, SharedLimiter};
use crate::routes;
use crate::secrets::SecretStore;
use crate::services::account_links::AccountLinkService;
use crate::services::background::BackgroundTasks;
use crate::services::connectors::ConnectorSettings;
use crate::services::documents::{DocumentService, Extractor};
use crate::services::embeddings::{EmbeddingServiceOptions, default_embedding_factory};
use crate::services::invalidations::{Invalidations, share_invalidations};
use crate::services::llm::{LlmServiceOptions, default_factory};
use crate::services::runs::{ApprovalNotifier, LessonCapture, RunExecutor};
use crate::services::sso::{SsoService, SystemDns};
use crate::services::{
    AccessService, ConnectorService, EmbeddingService, LlmService, RunEventBus, RunService,
};

const BODY_LIMIT_BYTES: usize = 1_000_000;
const GLOBAL_RATE_LIMIT_PER_MINUTE: u32 = 600;
/// Failed logins allowed per account before it is throttled for the window.
const FAILED_LOGINS_PER_ACCOUNT: u32 = 10;
pub(crate) const FAILED_LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Password reset links an address may ask for in the window.
const RESET_REQUESTS_PER_ADDRESS: u32 = 3;
pub(crate) const RESET_REQUEST_WINDOW: Duration = Duration::from_secs(60 * 60);

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
    /// The documents companies deposit, read by the extraction process
    /// (ADR-095).
    pub documents: Arc<DocumentService>,
    /// What this process changed in the caches, announced to the others.
    pub invalidations: Invalidations,
    pub runs: RunService,
    /// Signing in through the companies' identity providers (ADR-063).
    pub sso: Arc<SsoService>,
    /// Accounts each person links at their company's providers (ADR-116).
    pub account_links: Arc<AccountLinkService>,
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

pub struct App {
    pub router: Router,
    pub services: AppState,
    /// What runs in the background: none of it but the relay in an `api`
    /// process, which executes no runs; no relay in a `worker`, which
    /// serves no event stream (ADR-085).
    background: Background,
}

#[derive(Default)]
struct Background {
    maintenance: Option<JoinHandle<()>>,
    indexing: Option<JoinHandle<()>>,
    scheduling: Option<JoinHandle<()>>,
    lease_renewal: Option<JoinHandle<()>>,
    lease_takeover: Option<JoinHandle<()>>,
    run_queue: Option<JoinHandle<()>>,
    event_relay: Option<JoinHandle<()>>,
    delta_outbox: Option<JoinHandle<()>>,
    invalidations: Option<JoinHandle<()>>,
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
    let (invalidations, outgoing) = Invalidations::new();

    let llm = Arc::new(LlmService::new(
        db.clone(),
        LlmServiceOptions {
            platform: provider,
            secrets: SecretStore::new(config.secrets.clone()),
            listed_endpoints: config.tenant_llm_endpoints.clone(),
            factory: options.provider_factory.unwrap_or_else(default_factory),
            prices: Arc::new(PriceTable::default()),
            invalidations: invalidations.clone(),
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
            invalidations: invalidations.clone(),
        },
    ));
    let access = Arc::new(AccessService::new(db.clone(), invalidations.clone()));
    let connectors = Arc::new(ConnectorService::new(
        db.clone(),
        SecretStore::new(config.secrets.clone()),
        ConnectorSettings {
            network: config.mcp_network,
            start_timeout: config.mcp_start_timeout,
        },
        invalidations.clone(),
    ));
    let sso = Arc::new(SsoService::new(
        db.clone(),
        config.secrets.clone(),
        options.sso_network.unwrap_or(config.mcp_network),
        &config.web_origin,
        options.dns.unwrap_or_else(SystemDns::shared),
    )?);
    let account_links = Arc::new(AccountLinkService::new(
        db.clone(),
        config.secrets.clone(),
        options.sso_network.unwrap_or(config.mcp_network),
        &config.web_origin,
        options.account_link_issuer,
    )?);
    let mailer = match options.mailer {
        Some(mailer) => mailer,
        None => match &config.mail {
            Some(settings) => Some(Arc::new(SmtpMailer::new(settings)?) as Arc<dyn Mailer>),
            None => None,
        },
    };
    let background = Arc::new(BackgroundTasks::default());
    let (bus, outbox) = RunEventBus::new();
    let bus = Arc::new(bus);
    let executor = RunExecutor {
        db: db.clone(),
        access: access.clone(),
        llm: llm.clone(),
        prices: llm.prices().clone(),
        embeddings: embeddings.clone(),
        lessons: LessonCapture::new(db.clone(), llm.clone(), background.clone()),
        connectors: connectors.clone(),
        account_links: account_links.clone(),
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
        login: SharedLimiter::per_minute(db.clone(), "login", config.login_rate_limit_per_minute),
        sso_addresses: SharedDistinctLimiter::per_minute(
            db.clone(),
            "sso_addresses",
            config.login_rate_limit_per_minute,
        ),
        sso_sign_in: SharedLimiter::per_minute(
            db.clone(),
            "sso_sign_in",
            config.login_rate_limit_per_minute.saturating_mul(2),
        ),
        failed_logins: SharedLimiter::new(
            db.clone(),
            "failed_logins",
            FAILED_LOGINS_PER_ACCOUNT,
            FAILED_LOGIN_WINDOW,
        ),
        reset_requests: SharedLimiter::new(
            db.clone(),
            "reset_requests",
            RESET_REQUESTS_PER_ADDRESS,
            RESET_REQUEST_WINDOW,
        ),
    };
    let documents = Arc::new(DocumentService::new(
        db.clone(),
        Extractor::new(config.document_extractor.clone()),
        config.document_quota_bytes,
        embeddings.clone(),
    ));
    let services = Arc::new(Services {
        documents,
        runs: RunService::new(
            db.clone(),
            access.clone(),
            executor,
            options.run_leases.unwrap_or_default(),
            !config.role.executes_runs(),
        ),
        sso,
        account_links,
        mailer,
        config,
        db: db.clone(),
        access,
        connectors,
        bus: bus.clone(),
        llm,
        embeddings,
        invalidations,
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

    let role = services.config.role;
    let mut background = Background {
        invalidations: Some(tokio::spawn(share_invalidations(
            services.clone(),
            outgoing,
        ))),
        ..Background::default()
    };
    if role.serves_http() {
        background.event_relay = Some(spawn_run_event_relay(services.clone()));
    }
    if role.executes_runs() {
        background.maintenance = Some(spawn_maintenance(services.clone()));
        background.indexing =
            (!options.manual_indexing).then(|| spawn_memory_indexing(services.clone()));
        background.scheduling =
            (!options.manual_scheduling).then(|| spawn_scheduled_tasks(services.clone()));
        background.lease_renewal = Some(spawn_lease_renewal(services.clone()));
        background.lease_takeover = Some(spawn_lease_takeover(services.clone()));
        background.run_queue = Some(spawn_run_queue(services.clone()));
        let listens = role.serves_http();
        background.delta_outbox = Some(spawn_delta_outbox(&services, outbox, listens));
    }
    Ok(App {
        router,
        services,
        background,
    })
}

/// Stops `task`, if any, at once.
async fn abort(task: Option<JoinHandle<()>>) {
    if let Some(task) = task {
        task.abort();
        let _ = task.await;
    }
}

/// Waits for `task`, if any, to stop by itself.
async fn wait(task: Option<JoinHandle<()>>) {
    if let Some(task) = task {
        let _ = task.await;
    }
}

impl App {
    /// Starts executing runs, side by side with any other worker, and takes
    /// up those whose executor stopped (ADR-053, ADR-086); how many. Once,
    /// at startup, once listening, before serving; refused in an `api`
    /// process.
    pub async fn recover_interrupted_runs(&self) -> anyhow::Result<usize> {
        let services = &self.services;
        services.runs.recover_interrupted(&services.shutdown).await
    }

    /// Ends event streams so that open connections can close.
    pub fn begin_shutdown(&self) {
        self.services.shutdown.send_replace(true);
    }

    /// Waits for background runs and requests' background work, then
    /// releases connectors and connections.
    pub async fn shutdown(self) {
        self.begin_shutdown();
        let background = self.background;
        // The streams it fed end now.
        abort(background.event_relay).await;
        // Ended before the runs are awaited: its expiry sweep may start one.
        abort(background.maintenance).await;
        // So is this one, which starts runs: it stops between two tasks,
        // never in the middle of firing one.
        wait(background.scheduling).await;
        // Nor do these: they take up or claim no other run once the
        // service stops.
        wait(background.lease_takeover).await;
        wait(background.run_queue).await;
        abort(background.indexing).await;
        self.services.runs.idle().await;
        // The runs ended: no lease left to renew, nor text to send.
        abort(background.lease_renewal).await;
        abort(background.delta_outbox).await;
        self.services.background.idle().await;
        // Nothing left to change in the caches.
        abort(background.invalidations).await;
        self.services.connectors.close().await;
        self.services.db.close().await;
    }
}
