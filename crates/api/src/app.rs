//! Composition root of the API: services, cross-cutting layers, routes and
//! lifecycle. No business rule lives here.

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use mimorya_agent::llm::create_provider;
use mimorya_connectors::slack::SLACK_API;
use mimorya_connectors::teams::TeamsEndpoints;
use mimorya_connectors::telegram::TELEGRAM_API;
use mimorya_db::{Database, PoolOptions};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub use crate::build_options::BuildOptions;
use crate::config::AppConfig;
use crate::mail::{Mailer, SmtpMailer};
use crate::plugins::RateLimits;
use crate::plugins::csrf::csrf_protection;
use crate::plugins::gateway_replies::spawn_gateway_replies;
use crate::plugins::maintenance::spawn_maintenance;
use crate::plugins::memory_indexing::spawn_memory_indexing;
use crate::plugins::rate_limit::GLOBAL_RATE_LIMIT_PER_MINUTE;
use crate::plugins::rate_limit::global_rate_limit;
use crate::plugins::request_context::request_context;
use crate::plugins::run_event_relay::{spawn_delta_outbox, spawn_run_event_relay};
use crate::plugins::run_leases::{spawn_lease_renewal, spawn_lease_takeover};
use crate::plugins::run_queue::spawn_run_queue;
use crate::plugins::scheduled_tasks::spawn_scheduled_tasks;
use crate::plugins::security_headers::security_headers;
use crate::plugins::session::resolve_identity;
use crate::routes;
use crate::secrets::SecretStore;
use crate::services::account_links::AccountLinkService;
use crate::services::background::BackgroundTasks;
use crate::services::connectors::ConnectorSettings;
use crate::services::documents::{DocumentService, Extractor};
use crate::services::embeddings::{EmbeddingServiceOptions, default_embedding_factory};
use crate::services::gateways::{GatewayService, GatewaySettings};
use crate::services::invalidations::{Invalidations, share_invalidations};
use crate::services::llm::{LlmServiceOptions, default_factory};
use crate::services::runs::{ApprovalNotifier, ConversationSummaries, LessonCapture, RunExecutor};
use crate::services::sso::{SsoService, SystemDns};
use crate::services::{
    AccessService, ConnectorService, EmbeddingService, LlmService, RunEventBus, RunService,
};

const BODY_LIMIT_BYTES: usize = 1_000_000;

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
    /// The conversation gateways of each company: its Slack application
    /// (ADR-125).
    pub gateways: Arc<GatewayService>,
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
    gateway_replies: Option<JoinHandle<()>>,
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
            prices: Arc::new(options.prices.unwrap_or_default()),
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
    let gateways = Arc::new(GatewayService::new(
        db.clone(),
        config.secrets.clone(),
        GatewaySettings {
            network: options.sso_network.unwrap_or(config.mcp_network),
            slack_api: options.slack_api.unwrap_or_else(|| SLACK_API.to_string()),
            telegram_api: (options.telegram_api).unwrap_or_else(|| TELEGRAM_API.to_string()),
            teams: (options.teams_api.as_deref())
                .map_or_else(TeamsEndpoints::microsoft, TeamsEndpoints::local),
            web_origin: config.web_origin.clone(),
            assistant_name: config.assistant_name.clone(),
        },
        background.clone(),
    )?);
    let (bus, outbox) = RunEventBus::new();
    let bus = Arc::new(bus);
    let shutdown = watch::channel(false).0;
    let executor = RunExecutor {
        db: db.clone(),
        access: access.clone(),
        llm: llm.clone(),
        prices: llm.prices().clone(),
        embeddings: embeddings.clone(),
        lessons: LessonCapture::new(db.clone(), llm.clone(), background.clone()),
        summaries: ConversationSummaries::new(
            db.clone(),
            access.clone(),
            llm.clone(),
            background.clone(),
        )
        .stopping_on(shutdown.subscribe()),
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
    let global = options.global_rate_limit_per_minute;
    let limits = RateLimits::new(&db, &config, global.unwrap_or(GLOBAL_RATE_LIMIT_PER_MINUTE))?;
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
        gateways,
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
        shutdown,
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
        background.gateway_replies =
            (!options.manual_gateway_replies).then(|| spawn_gateway_replies(services.clone()));
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
        // A Slack message being answered may start a run as it ends: awaited
        // first, so that its run is awaited with the others.
        self.services.background.idle().await;
        self.services.runs.idle().await;
        // The runs ended: what remains to answer in Slack is answered by
        // the next process that starts.
        abort(background.gateway_replies).await;
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
