//! Shared harness for the API integration tests: a fresh test database (once
//! per test binary), an app backed by the scripted model or without any
//! model, and helpers to sign in as a demo user and send requests with the
//! session cookie.
#![allow(dead_code)] // not every test binary uses every helper

pub mod access;
pub mod account_links;
pub mod accounts;
pub mod approver_notices;
pub mod audit_chain;
pub mod audit_exports;
mod client;
pub mod companies;
pub mod connector_oauth;
pub mod connector_sessions;
pub mod conversation_links;
pub mod delegations;
pub mod dns;
pub mod document_search;
pub mod documents;
pub mod embeddings;
pub mod erasure;
pub mod evaluation_passes;
mod fixtures;
pub mod gateway_apps;
pub mod gateways;
pub mod held_token;
pub mod hooks;
pub mod identity_provider;
pub mod idle_conversations;
pub mod impostor;
pub mod knowledge_shares;
pub mod leases;
pub mod lessons;
pub mod limits;
pub mod link_revocations;
pub mod llm;
pub mod locks;
pub mod mail;
pub mod mailbox;
pub mod manager_approvals;
pub mod mcp;
pub mod mission_notices;
pub mod missions;
pub mod notification_choices;
pub mod old_audit;
pub mod owner;
pub mod person_authorizations;
pub mod person_identity;
pub mod person_renewals;
pub mod personal_belongings;
pub mod personal_export;
pub mod planning;
pub mod policy_proposal_notices;
pub mod policy_proposals;
pub mod privileges;
pub mod queue;
pub mod recovery;
pub mod refusals;
pub mod relay;
pub mod relay_faults;
pub mod resets;
pub mod rest;
pub mod retention;
pub mod routes;
pub mod runs;
pub mod saml_provider;
pub mod scheduled;
pub mod scheduled_notices;
pub mod scim;
mod sign_in;
pub mod skill_evaluations;
pub mod skill_proposal_notices;
pub mod skill_proposals;
pub mod slack_companies;
pub mod slack_conversations;
pub mod slack_validations;
pub mod slow_starts;
pub mod sso;
pub mod summaries;
pub mod suspension;
pub mod sweeps;
pub mod teams;
pub mod teams_conversations;
pub mod teams_validations;
pub mod telegram;
pub mod telegram_conversations;
pub mod telegram_validations;
pub mod token_endpoints;
pub mod usage_records;

pub use client::{Client, TestResponse};
use fixtures::urls;
pub use fixtures::{PASSWORD, WEB_ORIGIN, app_url, email, next_monday, owner_url, week};
use mail::RecordingMailer;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mimorya_agent::llm::{EmbeddingConfig, EmbeddingProvider, LlmProvider, ProviderConfig};
use mimorya_agent::testing::{ConceptEmbedder, ScriptedProvider, ScriptedStep};
use mimorya_api::mail::Mailer;
use mimorya_api::services::EmbeddingFactory;
use mimorya_api::services::runs::LeaseTiming;
use mimorya_api::{App, AppConfig, BuildOptions, build_app};

/// How a test app differs from `start`'s.
#[derive(Default)]
struct Setup<'a> {
    env: &'a [(&'a str, &'a str)],
    /// E-mails recorded in `mail`, instead of no mailer.
    record_mail: bool,
    background_indexing: bool,
    background_scheduling: bool,
    run_leases: Option<LeaseTiming>,
    /// Slack's Web API the gateways reach, a local one (ADR-125).
    slack_api: Option<String>,
    /// Telegram's Bot API the bots reach, a local one (ADR-133).
    telegram_api: Option<String>,
    /// Microsoft as the Teams bots reach it, a local one (ADR-138).
    teams_api: Option<String>,
    /// The scripted model priced (`llm::SCRIPTED_CALL_MICROS` a call).
    priced: bool,
}

pub struct TestApi {
    pub app: App,
    /// The scripted model, when the app has one.
    pub model: Option<Arc<ScriptedProvider>>,
    /// Serves the runs of a company that chose its own provider.
    pub company_model: Arc<ScriptedProvider>,
    /// Reviews the answers of a company that chose `llm::REVIEW_MODEL` (ADR-108).
    pub review_model: Arc<ScriptedProvider>,
    /// Every configuration a company provider was built from (key included).
    pub built: Arc<Mutex<Vec<ProviderConfig>>>,
    /// What the app e-mailed, when started with `start_with_mailer`.
    pub mail: Arc<RecordingMailer>,
    /// The DNS a domain's proof is read from (ADR-069).
    pub dns: Arc<dns::FakeDns>,
    /// Embeds the memory of a company that chose a model (ADR-074).
    pub embedder: Arc<ConceptEmbedder>,
    /// Every configuration an embedding model was built from (key included).
    pub embedders_built: Arc<Mutex<Vec<EmbeddingConfig>>>,
}

impl TestApi {
    /// App backed by the scripted model; queue its answers with `script`.
    pub async fn start() -> Self {
        Self::start_with(&[]).await
    }

    /// Same, with configuration variables overridden.
    pub async fn start_with(env: &[(&str, &str)]) -> Self {
        Self::scripted(Setup {
            env,
            ..Setup::default()
        })
        .await
    }

    /// Like `start`, scheduled tasks fired every minute as in production,
    /// once interrupted runs are taken up, not on demand.
    pub async fn start_scheduling() -> Self {
        Self::scripted(Setup {
            background_scheduling: true,
            ..Setup::default()
        })
        .await
    }

    /// Like `start_with`, memories indexed in the background as in
    /// production, not on demand.
    pub async fn start_indexing(env: &[(&str, &str)]) -> Self {
        Self::scripted(Setup {
            env,
            background_indexing: true,
            ..Setup::default()
        })
        .await
    }

    /// Like `start`, run leases lasting, renewed and taken up as `timing`
    /// says (ADR-083).
    pub async fn start_with_leases(timing: LeaseTiming) -> Self {
        Self::scripted(Setup {
            run_leases: Some(timing),
            ..Setup::default()
        })
        .await
    }

    /// App backed by the scripted model, priced: each call costs
    /// `llm::SCRIPTED_CALL_MICROS`.
    pub async fn start_priced() -> Self {
        Self::scripted(Setup {
            priced: true,
            ..Setup::default()
        })
        .await
    }

    /// Same, with e-mails recorded in `mail` instead of sent.
    pub async fn start_with_mailer() -> Self {
        Self::start_with_mailer_and(&[]).await
    }

    /// Same, with configuration variables overridden as well.
    pub async fn start_with_mailer_and(env: &[(&str, &str)]) -> Self {
        Self::scripted(Setup {
            env,
            record_mail: true,
            ..Setup::default()
        })
        .await
    }

    /// App without any model provider configured.
    pub async fn without_llm() -> Self {
        Self::build(None, None, Setup::default()).await
    }

    /// App backed by the scripted model, set up as `setup` says.
    async fn scripted(setup: Setup<'_>) -> Self {
        let model = Arc::new(ScriptedProvider::new(Vec::new()));
        let provider: Arc<dyn LlmProvider> = model.clone();
        Self::build(Some(provider), Some(model), setup).await
    }

    async fn build(
        provider: Option<Arc<dyn LlmProvider>>,
        model: Option<Arc<ScriptedProvider>>,
        setup: Setup<'_>,
    ) -> Self {
        let Setup {
            env,
            record_mail,
            background_indexing,
            background_scheduling,
            run_leases,
            slack_api,
            telegram_api,
            teams_api,
            priced,
        } = setup;
        limits::forget_counts().await;
        let mut vars: HashMap<&str, String> = HashMap::from([
            ("MIMORYA_ENV", "test".to_string()),
            ("LOG_LEVEL", "off".to_string()),
            ("DATABASE_APP_URL", urls().app.clone()),
            ("WEB_ORIGIN", WEB_ORIGIN.to_string()),
            ("LLM_PROVIDER", "none".to_string()),
            ("LOGIN_RATE_LIMIT_PER_MINUTE", "1000".to_string()),
            // The extraction process cargo builds beside the tests (ADR-095).
            (
                "MIMORYA_EXTRACTOR",
                env!("CARGO_BIN_EXE_mimorya-extract").to_string(),
            ),
        ]);
        vars.extend(env.iter().map(|(k, v)| (*k, (*v).to_string())));
        let config = AppConfig::from_vars(|key| vars.get(key).cloned()).expect("config");
        let company_model = Arc::new(ScriptedProvider::new(Vec::new()));
        let review_model = Arc::new(ScriptedProvider::new(Vec::new()));
        let built = Arc::new(Mutex::new(Vec::new()));
        let factory =
            llm::company_factory(company_model.clone(), review_model.clone(), built.clone());
        let embedder = Arc::new(ConceptEmbedder::new(embeddings::CONCEPTS));
        let embedders_built = Arc::new(Mutex::new(Vec::new()));
        let embedding_factory: EmbeddingFactory = {
            let (embedder, built) = (embedder.clone(), embedders_built.clone());
            Arc::new(move |config: &EmbeddingConfig| {
                built.lock().expect("built").push(config.clone());
                let provider: Arc<dyn EmbeddingProvider> = embedder.clone();
                Ok(provider)
            })
        };
        let mail = Arc::new(RecordingMailer::default());
        let dns = Arc::new(dns::FakeDns::default());
        let options = BuildOptions {
            provider: Some(provider),
            provider_factory: Some(factory),
            embedding_factory: Some(embedding_factory),
            manual_indexing: !background_indexing,
            // Fired by the tests (`fire_due_tasks`), when they make a task
            // due, unless started with `start_scheduling`.
            manual_scheduling: !background_scheduling,
            // Answered in Slack by the tests (`reply_to_finished_runs`).
            manual_gateway_replies: true,
            run_leases,
            mailer: record_mail.then(|| Some(mail.clone() as Arc<dyn Mailer>)),
            sso_network: None,
            global_rate_limit_per_minute: None,
            dns: Some(dns.clone()),
            account_link_issuer: None,
            slack_api,
            telegram_api,
            teams_api,
            prices: priced.then(llm::scripted_prices),
            connector_test_servers: Vec::new(),
        };
        let app = build_app(config, options).await.expect("app");
        Self {
            app,
            model,
            company_model,
            review_model,
            built,
            mail,
            dns,
            embedder,
            embedders_built,
        }
    }

    pub fn script(&self, steps: Vec<ScriptedStep>) {
        self.model.as_ref().expect("scripted model").push(steps);
    }

    /// Waits for every background run, and for the work requests left in
    /// the background (reset links, audits of denied logins).
    pub async fn idle(&self) {
        self.app.services.runs.idle().await;
        self.app.services.background.idle().await;
    }
}
