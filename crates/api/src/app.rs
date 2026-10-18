//! Composition root of the API: services, cross-cutting layers, routes and
//! lifecycle. No business rule lives here.

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::from_fn_with_state;
use mimorya_agent::llm::{LlmProvider, ModelRouter, PriceTable, create_provider};
use mimorya_db::{Database, PoolOptions};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::config::AppConfig;
use crate::plugins::csrf::csrf_protection;
use crate::plugins::rate_limit::global_rate_limit;
use crate::plugins::request_context::request_context;
use crate::plugins::session::resolve_identity;
use crate::plugins::stale_run_sweeper::spawn_stale_run_sweeper;
use crate::plugins::{RateLimiter, RateLimits};
use crate::routes;
use crate::services::runs::RunExecutor;
use crate::services::{AccessService, ConnectorService, RunEventBus, RunService};

const BODY_LIMIT_BYTES: usize = 1_000_000;
const GLOBAL_RATE_LIMIT_PER_MINUTE: u32 = 600;

pub struct Services {
    pub config: AppConfig,
    pub db: Database,
    pub access: Arc<AccessService>,
    pub connectors: Arc<ConnectorService>,
    pub bus: Arc<RunEventBus>,
    pub runs: RunService,
    pub limits: RateLimits,
    /// Flips to `true` when the server stops: long-lived streams end.
    pub shutdown: watch::Sender<bool>,
}

pub type AppState = Arc<Services>;

#[derive(Default)]
pub struct BuildOptions {
    /// Replaces the provider built from configuration (tests); `Some(None)`
    /// runs the server without any provider.
    pub provider: Option<Option<Arc<dyn LlmProvider>>>,
}

pub struct App {
    pub router: Router,
    pub services: AppState,
    sweeper: JoinHandle<()>,
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

    let access = Arc::new(AccessService::new(db.clone()));
    let connectors = Arc::new(ConnectorService::new(db.clone()));
    let bus = Arc::new(RunEventBus::default());
    let executor = RunExecutor {
        db: db.clone(),
        router: provider.map(ModelRouter::new),
        prices: Arc::new(PriceTable::default()),
        connectors: connectors.clone(),
        bus: bus.clone(),
        assistant_name: config.assistant_name.clone(),
        llm_judge: config.llm_judge,
    };
    let limits = RateLimits {
        global: RateLimiter::per_minute(GLOBAL_RATE_LIMIT_PER_MINUTE),
        login: RateLimiter::per_minute(config.login_rate_limit_per_minute),
    };
    let services = Arc::new(Services {
        runs: RunService::new(db.clone(), access.clone(), executor),
        config,
        db: db.clone(),
        access,
        connectors,
        bus,
        limits,
        shutdown: watch::channel(false).0,
    });

    // Layers run from the last added to the first: context, rate limit,
    // CSRF, then session resolution.
    let router = routes::router()
        .layer(from_fn_with_state(services.clone(), resolve_identity))
        .layer(from_fn_with_state(services.clone(), csrf_protection))
        .layer(from_fn_with_state(services.clone(), global_rate_limit))
        .layer(from_fn_with_state(services.clone(), request_context))
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .with_state(services.clone());

    Ok(App {
        router,
        services,
        sweeper: spawn_stale_run_sweeper(db),
    })
}

impl App {
    /// Ends event streams so that open connections can close.
    pub fn begin_shutdown(&self) {
        self.services.shutdown.send_replace(true);
    }

    /// Waits for background runs, then releases connectors and connections.
    pub async fn shutdown(self) {
        self.begin_shutdown();
        self.sweeper.abort();
        self.services.runs.idle().await;
        self.services.connectors.close().await;
        self.services.db.close().await;
    }
}
