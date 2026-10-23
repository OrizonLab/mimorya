//! Shared harness for the API integration tests: a fresh test database (once
//! per test binary), an app backed by the scripted model or without any
//! model, and helpers to sign in as a demo user and send requests with the
//! session cookie.
#![allow(dead_code)] // not every test binary uses every helper

pub mod access;
pub mod accounts;
mod client;
pub mod companies;
pub mod delegations;
pub mod dns;
pub mod embeddings;
pub mod identity_provider;
pub mod leases;
pub mod lessons;
pub mod llm;
pub mod locks;
pub mod mail;
pub mod manager_approvals;
pub mod mcp;
pub mod planning;
pub mod privileges;
pub mod recovery;
pub mod refusals;
pub mod relay;
pub mod relay_faults;
pub mod resets;
pub mod runs;
pub mod saml_provider;
pub mod scheduled;
pub mod scheduled_notices;
pub mod sso;
pub mod sweeps;

pub use client::{Client, TestResponse};
use mail::RecordingMailer;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::{Days, Utc};
use mimorya_agent::llm::{EmbeddingConfig, EmbeddingProvider, LlmProvider, ProviderConfig};
use mimorya_agent::testing::{ConceptEmbedder, ScriptedProvider, ScriptedStep};
use mimorya_api::auth::credentials::hash_password;
use mimorya_api::mail::Mailer;
use mimorya_api::services::runs::LeaseTiming;
use mimorya_api::services::{EmbeddingFactory, ProviderFactory};
use mimorya_api::{App, AppConfig, BuildOptions, build_app};
use mimorya_connectors::demo_interventions::dates::{iso_date, monday_of};
use mimorya_core::demo;
use mimorya_db::testing::{TestDatabaseUrls, reset_test_database};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

pub const PASSWORD: &str = "mot-de-passe-de-test";
pub const WEB_ORIGIN: &str = "http://localhost:3000";

static URLS: OnceLock<TestDatabaseUrls> = OnceLock::new();

/// The owner role's URL, for what the application role may not write.
pub fn owner_url() -> &'static str {
    &urls().owner
}

/// The application role's URL (row-level security applies).
pub fn app_url() -> &'static str {
    &urls().app
}

/// Resets the test database on first use (in a dedicated runtime, so that
/// each test keeps its own).
fn urls() -> &'static TestDatabaseUrls {
    URLS.get_or_init(|| {
        std::thread::spawn(|| {
            let hash = hash_password(PASSWORD).expect("password hash");
            tokio::runtime::Runtime::new()
                .expect("runtime")
                .block_on(reset_test_database(Some(hash)))
                .expect("test database reset")
        })
        .join()
        .expect("setup thread")
    })
}

pub fn email(user_id: Uuid) -> String {
    demo::users()
        .into_iter()
        .find(|u| u.id == user_id)
        .expect("demo user")
        .email
}

/// Monday to Friday of the current week, where the demo interventions are dated.
pub fn week() -> Value {
    let monday = monday_of(Utc::now().date_naive());
    json!({ "from": iso_date(monday), "to": iso_date(monday + Days::new(4)) })
}

pub fn next_monday() -> String {
    iso_date(monday_of(Utc::now().date_naive()) + Days::new(7))
}

/// How a test app differs from `start`'s.
#[derive(Default)]
struct Setup<'a> {
    env: &'a [(&'a str, &'a str)],
    /// E-mails recorded in `mail`, instead of no mailer.
    record_mail: bool,
    background_indexing: bool,
    background_scheduling: bool,
    run_leases: Option<LeaseTiming>,
}

pub struct TestApi {
    pub app: App,
    /// The scripted model, when the app has one.
    pub model: Option<Arc<ScriptedProvider>>,
    /// Serves the runs of a company that chose its own provider.
    pub company_model: Arc<ScriptedProvider>,
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
        } = setup;
        let mut vars: HashMap<&str, String> = HashMap::from([
            ("MIMORYA_ENV", "test".to_string()),
            ("LOG_LEVEL", "off".to_string()),
            ("DATABASE_APP_URL", urls().app.clone()),
            ("WEB_ORIGIN", WEB_ORIGIN.to_string()),
            ("LLM_PROVIDER", "none".to_string()),
            ("LOGIN_RATE_LIMIT_PER_MINUTE", "1000".to_string()),
        ]);
        vars.extend(env.iter().map(|(k, v)| (*k, (*v).to_string())));
        let config = AppConfig::from_vars(|key| vars.get(key).cloned()).expect("config");
        let company_model = Arc::new(ScriptedProvider::new(Vec::new()));
        let built = Arc::new(Mutex::new(Vec::new()));
        let factory: ProviderFactory = {
            let (company_model, built) = (company_model.clone(), built.clone());
            Arc::new(move |config: &ProviderConfig| {
                built.lock().expect("built").push(config.clone());
                let provider: Arc<dyn LlmProvider> = company_model.clone();
                Ok(provider)
            })
        };
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
            run_leases,
            mailer: record_mail.then(|| Some(mail.clone() as Arc<dyn Mailer>)),
            sso_network: None,
            global_rate_limit_per_minute: None,
            dns: Some(dns.clone()),
        };
        let app = build_app(config, options).await.expect("app");
        Self {
            app,
            model,
            company_model,
            built,
            mail,
            dns,
            embedder,
            embedders_built,
        }
    }

    /// Requests without a session (pages before sign-in).
    pub fn anonymous(&self) -> Client<'_> {
        self.as_user("")
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

    pub async fn send(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .app
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("infallible router");
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        TestResponse {
            status,
            headers,
            body,
        }
    }

    pub async fn try_login(&self, email: &str, password: &str) -> TestResponse {
        self.try_login_from(email, password, None).await
    }

    /// A login claiming to come from `forwarded_for` (`X-Forwarded-For`).
    pub async fn try_login_from(
        &self,
        email: &str,
        password: &str,
        forwarded_for: Option<&str>,
    ) -> TestResponse {
        let mut builder = Request::post("/auth/login")
            .header("x-mimorya-request", "1")
            .header("origin", WEB_ORIGIN)
            .header("content-type", "application/json");
        if let Some(address) = forwarded_for {
            builder = builder.header("x-forwarded-for", address);
        }
        let payload = json!({ "email": email, "password": password });
        self.send(builder.body(Body::from(payload.to_string())).unwrap())
            .await
    }

    /// Signs in as a demo user and returns the session token.
    pub async fn login(&self, user_id: Uuid) -> String {
        let response = self.try_login(&email(user_id), PASSWORD).await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        let cookie = response.headers["set-cookie"].to_str().unwrap().to_string();
        assert!(cookie.contains("HttpOnly"));
        let token = cookie
            .strip_prefix("mimorya_session=")
            .and_then(|rest| rest.split(';').next())
            .expect("session cookie");
        token.to_string()
    }

    pub fn as_user(&self, session: &str) -> Client<'_> {
        Client::new(self, session)
    }

    pub async fn sign_in(&self, user_id: Uuid) -> Client<'_> {
        let session = self.login(user_id).await;
        self.as_user(&session)
    }

    /// Signs in with explicit credentials (accounts created by a test).
    pub async fn sign_in_with(&self, email: &str, password: &str) -> Client<'_> {
        let response = self.try_login(email, password).await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        self.session_from(&response)
    }

    /// The session opened by a successful login response.
    pub fn session_from(&self, login: &TestResponse) -> Client<'_> {
        let cookie = login.headers["set-cookie"].to_str().unwrap();
        let token = cookie
            .strip_prefix("mimorya_session=")
            .and_then(|rest| rest.split(';').next())
            .expect("session cookie");
        self.as_user(token)
    }
}
