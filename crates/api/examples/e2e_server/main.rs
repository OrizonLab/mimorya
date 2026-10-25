//! The API as the end-to-end tests drive it (`apps/web/e2e`): the real
//! server and database, but a scripted model playing the reference scenario
//! (ADR-031) and e-mails written to a directory instead of sent. An example,
//! not a binary: it is never built into a release.
//!
//!   cargo run -p mimorya-api --example e2e_server
//!
//! Reads the usual configuration (`DATABASE_APP_URL`, `WEB_ORIGIN`…) and
//! `E2E_MAIL_DIR`, where each e-mail becomes a JSON file (the ones a former
//! start left are deleted). An OpenID Connect provider listens on
//! 127.0.0.1:4100 (`IDP_ADDRESS`, which the tests declare), signing in
//! whoever the sign-in names, and a SAML provider on 127.0.0.1:4101
//! (`SAML_IDP_ADDRESS`), signing in whoever is typed at its page; identity
//! providers alone are reached on the private network, connectors keep to
//! the public one. The DNS a domain's proof is read from is the tests' own,
//! where they publish records on 127.0.0.1:4102 (`DNS_ADDRESS`, ADR-069).
//! A company's embedding model is a test one, whatever it chooses: no
//! provider is ever reached (ADR-074). A finished run reread for a lesson
//! always yields the same one (`lessons`, ADR-076). A mission is played
//! apart, stage after stage (`missions`, ADR-089), and so is a question on
//! the company's documents (`documents`, ADR-096), read by the extraction
//! process the tests build (`MIMORYA_EXTRACTOR`, ADR-095). One pass of the tests
//! per start: the model's script and the demo company are used up, and the
//! rate limit windows a former start left are forgotten (ADR-088). Like
//! `mimorya-api`, once listening it takes up the runs whose executor
//! stopped, among them the one it leaves itself without a lease
//! (`interrupted`, ADR-053, ADR-086); then fires the scheduled tasks due every
//! minute, among them one a stop made miss its occurrence (`scheduled`,
//! ADR-080), and claims the queued runs, among them one it leaves itself
//! and holds until the browser followed it (`queued`, ADR-085).

mod dns_server;
mod documents;
#[allow(dead_code)] // the API's tests use the rest
#[path = "../../tests/support/dns.rs"]
mod fake_dns;
#[allow(dead_code)] // the API's tests use the rest
#[path = "../../tests/support/identity_provider.rs"]
mod identity_provider;
mod interrupted;
mod lessons;
mod mailbox;
mod missions;
mod queued;
mod saml_idp;
#[allow(dead_code)] // the API's tests use the rest
#[path = "../../tests/support/saml_provider.rs"]
mod saml_provider;
mod scenario;
mod scheduled;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use mimorya_agent::llm::{EmbeddingProvider, LlmProvider};
use mimorya_agent::testing::{ConceptEmbedder, ScriptedProvider, ScriptedStep};
use mimorya_api::mail::Mailer;
use mimorya_api::services::{EmbeddingFactory, ProviderFactory};
use mimorya_api::{AppConfig, BuildOptions, build_app, log_filter};
use mimorya_connectors::mcp::NetworkPolicy;
use mimorya_db::repositories::rate_limits::purge_rate_limits;

/// Reference scenarios queued for the platform model, by the hour of their
/// follow-up: one per run that converses, in the order they run (the
/// interrupted run taken up at startup, which `interrupted-run.spec.ts`
/// refuses, then the one of `lessons.spec.ts`, that of
/// `manager-absence.spec.ts`, the two of `manager-approvals.spec.ts`,
/// `reference-scenario.spec.ts`, the three of `timeouts.spec.ts` and
/// `unavailable-connectors.spec.ts`). Only the one of
/// `reference-scenario.spec.ts` is validated: the others are refused (or
/// withdrawn), so their slots, which may overlap (two hours each, within
/// the working day, 8 h – 17 h), are never taken.
const SCENARIO_HOURS: [u32; 10] = [10, 10, 12, 14, 12, 13, 15, 9, 11, 8];

/// Where the identity provider listens: `single-sign-on.spec.ts` declares it.
const IDP_ADDRESS: &str = "127.0.0.1:4100";

/// Where the SAML provider listens: `saml-sign-on.spec.ts` declares it.
const SAML_IDP_ADDRESS: &str = "127.0.0.1:4101";

/// Where the tests publish DNS records (ADR-069).
const DNS_ADDRESS: &str = "127.0.0.1:4102";

/// Requests per minute from the web relay, which every test goes through.
const GLOBAL_RATE_LIMIT_PER_MINUTE: u32 = 100_000;

/// Company providers' answers to their connection test.
const CONNECTION_TESTS: usize = 20;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env()?;
    tracing_subscriber::fmt()
        .with_env_filter(log_filter(&config.log_level)?)
        .init();
    let mail_dir = PathBuf::from(
        std::env::var("E2E_MAIL_DIR").unwrap_or_else(|_| "target/e2e/mail".to_string()),
    );
    mailbox::empty(&mail_dir)?;

    let model = Arc::new(ScriptedProvider::new(Vec::new()));
    for hour in SCENARIO_HOURS {
        model.push(scenario::reference(hour));
    }
    let conversations = Arc::new(queued::WithQueued::new(model));
    let documents = Arc::new(documents::WithDocuments::new(conversations));
    let missions = Arc::new(missions::WithMissions::new(documents));
    let platform: Arc<dyn LlmProvider> = Arc::new(lessons::WithLessons::new(missions));
    // A company's own provider answers « OK » (its connection test).
    let factory: ProviderFactory = Arc::new(|_config| {
        let provider: Arc<dyn LlmProvider> = Arc::new(ScriptedProvider::new(
            (0..CONNECTION_TESTS)
                .map(|_| ScriptedStep::text("OK"))
                .collect(),
        ));
        Ok(provider)
    });
    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(ConceptEmbedder::new(&[]));
    let embedding_factory: EmbeddingFactory = Arc::new(move |_config| Ok(embedder.clone()));
    let mailer: Arc<dyn Mailer> = Arc::new(mailbox::DirectoryMailer::new(mail_dir));
    let idp = identity_provider::MockIdentityProvider::start_on(IDP_ADDRESS).await;
    tracing::info!(
        issuer = idp.issuer,
        "End-to-end identity provider listening"
    );
    let saml_idp = saml_idp::start_on(SAML_IDP_ADDRESS).await?;
    tracing::info!(address = saml_idp, "End-to-end SAML provider listening");
    let dns = Arc::new(fake_dns::FakeDns::default());
    dns_server::start_on(DNS_ADDRESS, dns.clone()).await?;
    let address = format!("{}:{}", config.host, config.port);
    let app = build_app(
        config,
        BuildOptions {
            provider: Some(Some(platform)),
            provider_factory: Some(factory),
            embedding_factory: Some(embedding_factory),
            manual_indexing: false,
            manual_scheduling: false,
            run_leases: None,
            mailer: Some(Some(mailer)),
            sso_network: Some(NetworkPolicy {
                allow_private_network: true,
                allow_stdio: false,
            }),
            // The browser's requests all come through the web relay, from one
            // address: the whole suite would share a minute's 600.
            global_rate_limit_per_minute: Some(GLOBAL_RATE_LIMIT_PER_MINUTE),
            dns: Some(dns),
        },
    )
    .await?;
    interrupted::leave(&app).await?;
    scheduled::leave(&app).await?;
    queued::leave(&app).await?;
    // A former start's windows, which a restart forgot when they were counted
    // in memory: the suite runs again within the hour (ADR-088).
    purge_rate_limits(app.services.db.pool(), Utc::now()).await?;
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let taken = app.recover_interrupted_runs().await?;
    tracing::info!(address, taken, "End-to-end API listening");
    axum::serve(
        listener,
        app.router
            .clone()
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
