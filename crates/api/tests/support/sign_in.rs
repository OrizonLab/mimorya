//! A test app's requests and sessions: sent through its router as a
//! browser would, signed in as a demo user or with an account's own
//! credentials, or without a session.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

use super::{Client, PASSWORD, TestApi, TestResponse, WEB_ORIGIN, email};

impl TestApi {
    /// Requests without a session (pages before sign-in).
    pub fn anonymous(&self) -> Client<'_> {
        self.as_user("")
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
