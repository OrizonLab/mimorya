//! Composition root of the API: services, cross-cutting layers, routes and
//! lifecycle; what runs in the background, in `background`. No business
//! rule lives here.

mod background;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use mimorya_agent::llm::create_provider;
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::slack::SLACK_API;
use mimorya_connectors::teams::TeamsEndpoints;
use mimorya_connectors::telegram::TELEGRAM_API;
use mimorya_db::{Database, PoolOptions};
use tokio::sync::watch;

use background::{Background, OnDemand};

pub use crate::build_options::BuildOptions;
use crate::config::AppConfig;
use crate::mail::{Mailer, SmtpMailer};
use crate::plugins::RateLimits;
use crate::plugins::csrf::csrf_protection;
use crate::plugins::rate_limit::GLOBAL_RATE_LIMIT_PER_MINUTE;
use crate::plugins::rate_limit::global_rate_limit;
use crate::plugins::request_context::request_context;
use crate::plugins::security_headers::security_headers;
use crate::plugins::session::resolve_identity;
use crate::routes;
use crate::secrets::SecretStore;
use crate::services::account_links::AccountLinkService;
use crate::services::audit_chain::ChainChecks;
use crate::services::background::BackgroundTasks;
use crate::services::connectors::ConnectorSettings;
use crate::services::document_labels::DocumentLabelService;
use crate::services::documents::{DocumentService, Extractor};
use crate::services::embeddings::{EmbeddingServiceOptions, default_embedding_factory};
use crate::services::gateways::{GatewayService, GatewaySettings};
use crate::services::invalidations::Invalidations;
use crate::services::llm::{DecisionConfig, LlmServiceOptions, default_factory};
use crate::services::personal_export::Exports;
use crate::services::runs::{ApprovalNotifier, ConversationSummaries, LessonCapture, RunExecutor};
use crate::services::skill_evaluations::EvaluationSlots;
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
    /// Their type and confidentiality classified by the decision service
    /// (ADR-213, D4).
    pub document_labels: Arc<DocumentLabelService>,
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
    /// The companies whose skill evaluations this process plays at once
    /// (ADR-157).
    pub evaluation_slots: EvaluationSlots,
    /// The verifications of their audit chain administrators ask for that
    /// this process runs at once (ADR-179).
    pub audit_checks: ChainChecks,
    /// The exports of their own data people ask for that this process
    /// makes at once (ADR-187).
    pub exports: Exports,
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
            // The escalation of a company the platform's model serves is
            // built with its configuration (ADR-209).
            platform_config: config.llm.clone(),
            secrets: SecretStore::new(config.secrets.clone()),
            listed_endpoints: config.tenant_llm_endpoints.clone(),
            factory: options.provider_factory.unwrap_or_else(default_factory),
            prices: Arc::new(options.prices.unwrap_or_default()),
            invalidations: invalidations.clone(),
            advisor_allowed: config.anthropic_advisor,
            decisions: DecisionConfig::new(&config, options.typesafe_api),
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
            network: NetworkPolicy {
                // Only a test builds a server with some (`BuildOptions`).
                test_servers: Vec::leak(options.connector_test_servers),
                test_roots: Vec::leak(options.connector_test_roots),
                ..config.mcp_network
            },
            start_timeout: config.mcp_start_timeout,
            start_patience: ConnectorSettings::patience(
                config.mcp_start_timeout,
                config.mcp_start_patience,
            ),
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
    let document_labels = Arc::new(DocumentLabelService::new(
        db.clone(),
        llm.clone(),
        invalidations.clone(),
    ));
    let documents = Arc::new(DocumentService::new(
        db.clone(),
        Extractor::new(config.document_extractor.clone()),
        config.document_quota_bytes,
        embeddings.clone(),
        document_labels.clone(),
    ));
    let services = Arc::new(Services {
        documents,
        document_labels,
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
        evaluation_slots: EvaluationSlots::default(),
        audit_checks: ChainChecks::default(),
        exports: Exports::default(),
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

    let on_demand = OnDemand {
        indexing: options.manual_indexing,
        document_labels: options.manual_document_labels,
        scheduling: options.manual_scheduling,
        gateway_replies: options.manual_gateway_replies,
    };
    let background = Background::start(&services, on_demand, outgoing, outbox);
    Ok(App {
        router,
        services,
        background,
    })
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
        self.background.stop(&self.services).await;
        self.services.connectors.close().await;
        self.services.db.close().await;
    }
}
