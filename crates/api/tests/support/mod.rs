//! Shared harness for the API integration tests: a fresh test database (once
//! per test binary), an app backed by the scripted model or without any
//! model, and helpers to sign in as a demo user and send requests with the
//! session cookie.
#![allow(dead_code)] // not every test binary uses every helper

pub mod planning;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use axum::body::{Body, Bytes, to_bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use chrono::{Days, Utc};
use mimorya_agent::llm::LlmProvider;
use mimorya_agent::testing::{ScriptedProvider, ScriptedStep};
use mimorya_api::auth::credentials::hash_password;
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

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl TestResponse {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|_| {
            panic!(
                "JSON body expected, got {:?}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn error_message(&self) -> String {
        self.json()["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }
}

pub struct TestApi {
    pub app: App,
    /// The scripted model, when the app has one.
    pub model: Option<Arc<ScriptedProvider>>,
}

impl TestApi {
    /// App backed by the scripted model; queue its answers with `script`.
    pub async fn start() -> Self {
        let model = Arc::new(ScriptedProvider::new(Vec::new()));
        let provider: Arc<dyn LlmProvider> = model.clone();
        Self::build(Some(provider), Some(model)).await
    }

    /// App without any model provider configured.
    pub async fn without_llm() -> Self {
        Self::build(None, None).await
    }

    async fn build(
        provider: Option<Arc<dyn LlmProvider>>,
        model: Option<Arc<ScriptedProvider>>,
    ) -> Self {
        let vars: HashMap<&str, String> = HashMap::from([
            ("MIMORYA_ENV", "test".to_string()),
            ("LOG_LEVEL", "off".to_string()),
            ("DATABASE_APP_URL", urls().app.clone()),
            ("WEB_ORIGIN", WEB_ORIGIN.to_string()),
            ("LLM_PROVIDER", "none".to_string()),
            ("LOGIN_RATE_LIMIT_PER_MINUTE", "1000".to_string()),
        ]);
        let config = AppConfig::from_vars(|key| vars.get(key).cloned()).expect("config");
        let options = BuildOptions {
            provider: Some(provider),
        };
        let app = build_app(config, options).await.expect("app");
        Self { app, model }
    }

    pub fn script(&self, steps: Vec<ScriptedStep>) {
        self.model.as_ref().expect("scripted model").push(steps);
    }

    /// Waits for every background run.
    pub async fn idle(&self) {
        self.app.services.runs.idle().await;
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
        let request = Request::post("/auth/login")
            .header("x-mimorya-request", "1")
            .header("origin", WEB_ORIGIN)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "email": email, "password": password }).to_string(),
            ))
            .unwrap();
        self.send(request).await
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
        Client {
            api: self,
            session: session.to_string(),
        }
    }

    pub async fn sign_in(&self, user_id: Uuid) -> Client<'_> {
        let session = self.login(user_id).await;
        self.as_user(&session)
    }
}

pub struct Client<'a> {
    api: &'a TestApi,
    pub session: String,
}

impl Client<'_> {
    fn request(&self, method: Method, url: &str) -> axum::http::request::Builder {
        let builder = Request::builder()
            .method(method.clone())
            .uri(url)
            .header("cookie", format!("mimorya_session={}", self.session));
        if method == Method::GET {
            builder
        } else {
            builder.header("x-mimorya-request", "1")
        }
    }

    pub async fn get(&self, url: &str) -> TestResponse {
        let request = self.request(Method::GET, url).body(Body::empty()).unwrap();
        self.api.send(request).await
    }

    pub async fn get_with(&self, url: &str, headers: &[(&str, &str)]) -> TestResponse {
        let mut builder = self.request(Method::GET, url);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        self.api.send(builder.body(Body::empty()).unwrap()).await
    }

    pub async fn post(&self, url: &str, payload: Value) -> TestResponse {
        let request = self
            .request(Method::POST, url)
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        self.api.send(request).await
    }

    pub async fn delete(&self, url: &str) -> TestResponse {
        let request = self
            .request(Method::DELETE, url)
            .body(Body::empty())
            .unwrap();
        self.api.send(request).await
    }
}
