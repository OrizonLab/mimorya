//! Composition root of the API: services, cross-cutting layers, routes and
//! lifecycle. No business rule lives here.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use mimorya_agent::llm::{LlmProvider, PriceTable, create_provider};
use mimorya_db::{Database, PoolOptions};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::config::AppConfig;
use crate::mail::{Mailer, SmtpMailer};
use crate::plugins::csrf::csrf_protection;
use crate::plugins::maintenance::spawn_maintenance;
use crate::plugins::rate_limit::global_rate_limit;
use crate::plugins::request_context::request_context;
use crate::plugins::security_headers::security_headers;
use crate::plugins::session::resolve_identity;
use crate::plugins::{RateLimiter, RateLimits};
use crate::routes;
use crate::secrets::SecretStore;
use crate::services::background::BackgroundTasks;
use crate::services::connectors::ConnectorSettings;
use crate::services::llm::{LlmServiceOptions, default_factory};
use crate::services::runs::{ApprovalNotifier, RunExecutor};
use crate::services::{
    AccessService, ConnectorService, LlmService, ProviderFactory, RunEventBus, RunService,
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
    pub runs: RunService,
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
    /// Replaces the mailer built from configuration (tests); `Some(None)`
    /// sends nothing.
    pub mailer: Option<Option<Arc<dyn Mailer>>>,
}

pub struct App {
    pub router: Router,
    pub services: AppState,
    maintenance: JoinHandle<()>,
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
    let access = Arc::new(AccessService::new(db.clone()));
    let connectors = Arc::new(ConnectorService::new(
        db.clone(),
        SecretStore::new(config.secrets.clone()),
        ConnectorSettings {
            network: config.mcp_network,
            start_timeout: config.mcp_start_timeout,
        },
    ));
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
        connectors: connectors.clone(),
        bus: bus.clone(),
        notifier: ApprovalNotifier::new(
            db.clone(),
            mailer.clone(),
            config.web_origin.clone(),
            config.assistant_name.clone(),
            background.clone(),
        ),
        assistant_name: config.assistant_name.clone(),
        llm_judge: config.llm_judge,
    };
    let limits = RateLimits {
        global: RateLimiter::per_minute(GLOBAL_RATE_LIMIT_PER_MINUTE),
        login: RateLimiter::per_minute(config.login_rate_limit_per_minute),
        failed_logins: RateLimiter::new(FAILED_LOGINS_PER_ACCOUNT, FAILED_LOGIN_WINDOW),
        reset_requests: RateLimiter::new(RESET_REQUESTS_PER_ADDRESS, RESET_REQUEST_WINDOW),
    };
    let services = Arc::new(Services {
        runs: RunService::new(db.clone(), access.clone(), executor),
        mailer,
        config,
        db: db.clone(),
        access,
        connectors,
        bus: bus.clone(),
        llm,
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
        services,
    })
}

impl App {
    /// Takes up again the runs a stop of the service left running (ADR-053);
    /// how many there were. Once, at startup, once listening; it fails while
    /// another process holds the run executor's lock.
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
        self.maintenance.abort();
        self.services.runs.idle().await;
        self.services.background.idle().await;
        self.services.connectors.close().await;
        self.services.db.close().await;
    }
}
