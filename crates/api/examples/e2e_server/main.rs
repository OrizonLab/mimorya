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
//! The companies' Slack gateways reach a local Slack on 127.0.0.1:4103
//! (`SLACK_ADDRESS`), which knows Léa's Slack user, lists what the
//! gateways posted (ADR-126) and is told the permissions its token has
//! (`POST /test/scopes`, ADR-132); what she writes there is answered apart
//! (`slack`, ADR-128). Their Telegram bots reach a local Telegram on
//! 127.0.0.1:4104 (`TELEGRAM_ADDRESS`), which lists what the bots wrote and
//! the webhook each was given (ADR-134); what she writes to the bot is
//! answered apart (`telegram`, ADR-135). Their Teams bots reach a local
//! Microsoft on 127.0.0.1:4105 (`TEAMS_ADDRESS`): Entra, the Bot
//! Framework's keys and a Teams service, which knows Léa's Teams account,
//! lists what the bots wrote and signs the activities the tests send
//! (ADR-139); what she writes to the bot is answered apart (`teams`,
//! ADR-140).
//! A company's embedding model is a test one, whatever it chooses: no
//! provider is ever reached (ADR-074). A finished run reread for a lesson
//! always yields the same one (`lessons`, ADR-076). A mission is played
//! apart, stage after stage (`missions`, ADR-089), and so is a question on
//! the company's documents (`documents`, ADR-096), read by the extraction
//! process the tests build (`MIMORYA_EXTRACTOR`, ADR-095), one handed to a
//! sub-agent (`sub_agents`, ADR-099), a skill the agent proposes
//! (`skill_proposals`, ADR-101), a skill's version tried on its case
//! (`skill_evaluations`, ADR-105), and what an inbound hook sends (`hooks`,
//! ADR-110). One pass of the tests
//! per start: the model's script and the demo company are used up, and the
//! rate limit windows a former start left are forgotten (ADR-088). Like
//! `mimorya-api`, once listening it takes up the runs whose executor
//! stopped, among them the one it leaves itself without a lease
//! (`interrupted`, ADR-053, ADR-086); then fires the scheduled tasks due every
//! minute, among them one a stop made miss its occurrence (`scheduled`,
//! ADR-080), and claims the queued runs, among them one it leaves itself
//! and holds until the browser followed it (`queued`, ADR-085). It also
//! leaves long conversations of Marc's whose earlier messages are
//! summarised (`summaries`, ADR-146).

mod dns_server;
mod documents;
#[allow(dead_code)] // the API's tests use the rest
#[path = "../../tests/support/dns.rs"]
mod fake_dns;
mod hooks;
#[allow(dead_code)] // the API's tests use the rest
#[path = "../../tests/support/identity_provider.rs"]
mod identity_provider;
mod interrupted;
mod lessons;
mod mailbox;
mod missions;
mod played;
mod queued;
mod rate_limits;
mod saml_idp;
#[allow(dead_code)] // the API's tests use the rest
#[path = "../../tests/support/saml_provider.rs"]
mod saml_provider;
mod scenario;
mod scheduled;
mod skill_evaluations;
mod skill_proposals;
mod slack;
mod sub_agents;
mod summaries;
mod teams;
mod telegram;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use mimorya_agent::llm::{EmbeddingProvider, LlmProvider};
use mimorya_agent::testing::{ConceptEmbedder, ScriptedProvider, ScriptedStep};
use mimorya_api::mail::Mailer;
use mimorya_api::services::{EmbeddingFactory, ProviderFactory};
use mimorya_api::{AppConfig, BuildOptions, build_app, log_filter};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::testing::{FakeSlack, FakeTeams, FakeTelegram};

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

/// Where Slack's Web API listens, and lists the messages the gateways posted:
/// `slack-gateway.spec.ts` reads them there (ADR-126).
const SLACK_ADDRESS: &str = "127.0.0.1:4103";
/// Where Telegram's Bot API listens, and lists what the bots wrote and the
/// webhook each was given: `telegram-gateway.spec.ts` reads them there
/// (ADR-134).
const TELEGRAM_ADDRESS: &str = "127.0.0.1:4104";
/// Where Microsoft listens for the Teams bots, lists what they wrote and
/// signs activities: `teams-gateway.spec.ts` reads and asks there (ADR-139).
const TEAMS_ADDRESS: &str = "127.0.0.1:4105";
/// Léa's Slack user, of the company's workspace, with her address.
const LEA_SLACK: &str = "U0LEAE2E";
/// Léa's Microsoft Entra object, of the test directory, and her name.
const LEA_TEAMS: &str = "3c4d5e6f-7a8b-4c9d-8e0f-1a2b3c4d5e6f";
const LEA_NAME: &str = "Léa Fontaine";
const LEA_ADDRESS: &str = "lea.fontaine@neo-maintenance.example";

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
    let received = Arc::new(hooks::WithHooks::new(documents));
    let written = Arc::new(slack::WithSlack::new(received));
    let chatted = Arc::new(telegram::WithTelegram::new(written));
    let met = Arc::new(teams::WithTeams::new(chatted));
    let delegations = Arc::new(sub_agents::WithSubAgents::new(met));
    let proposals = Arc::new(skill_proposals::WithSkillProposals::new(delegations));
    let missions = Arc::new(missions::WithMissions::new(proposals));
    // In front of the missions, which pass any answer they are asked to judge.
    let evaluations = Arc::new(skill_evaluations::WithSkillEvaluations::new(missions));
    let platform: Arc<dyn LlmProvider> = Arc::new(lessons::WithLessons::new(evaluations));
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
    let slack = FakeSlack::start_on(SLACK_ADDRESS).await;
    slack.add_person(LEA_SLACK, LEA_ADDRESS);
    tracing::info!(url = slack.url, "End-to-end Slack listening");
    let telegram = FakeTelegram::start_on(TELEGRAM_ADDRESS).await;
    tracing::info!(url = telegram.url, "End-to-end Telegram listening");
    let teams = FakeTeams::start_on(TEAMS_ADDRESS).await;
    teams.add_person(LEA_TEAMS, LEA_NAME, LEA_ADDRESS);
    tracing::info!(url = teams.url, "End-to-end Microsoft listening");
    let address = format!("{}:{}", config.host, config.port);
    let app = build_app(
        config,
        BuildOptions {
            provider: Some(Some(platform)),
            provider_factory: Some(factory),
            embedding_factory: Some(embedding_factory),
            manual_indexing: false,
            manual_scheduling: false,
            manual_gateway_replies: false,
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
            // Its people link their accounts at the local provider (ADR-118).
            account_link_issuer: Some(idp.issuer.clone()),
            // Its gateways reach the local Slack (ADR-126).
            slack_api: Some(slack.url.clone()),
            // Its bot reaches the local Telegram (ADR-134).
            telegram_api: Some(telegram.url.clone()),
            // Its Teams bot reaches the local Microsoft (ADR-139).
            teams_api: Some(teams.url.clone()),
        },
    )
    .await?;
    interrupted::leave(&app).await?;
    scheduled::leave(&app).await?;
    queued::leave(&app).await?;
    summaries::leave(&app).await?;
    rate_limits::forget().await?;
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
