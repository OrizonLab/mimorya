//! The second factor in tests (ADR-200): an account enrolled as its app
//! would be, the codes that app shows, and a sign-in that waits for one,
//! completed with the challenge's cookie as a browser sends it.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use mimorya_core::second_factor::{DIGITS, code_at, read_base32, step_at};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, PASSWORD, TestApi, TestResponse, WEB_ORIGIN, owner_url};

/// An account's app: its secret, and the step of the last code it gave.
pub struct App {
    secret: Vec<u8>,
    step: i64,
}

impl App {
    /// The app's code for the next step after its last one: a code the
    /// server accepts once, within the minute.
    pub fn next_code(&mut self) -> String {
        self.step += 1;
        format!("{:06}", code_at(&self.secret, self.step, DIGITS))
    }

    /// The code of its last step again: one the server already took.
    pub fn last_code(&self) -> String {
        format!("{:06}", code_at(&self.secret, self.step, DIGITS))
    }
}

/// Six digits the app of `secret` shows at no step near now.
pub fn not_its_code(secret: &[u8]) -> String {
    let now = step_at(Utc::now().timestamp());
    let near: Vec<u32> = (now - 2..=now + 2)
        .map(|step| code_at(secret, step, DIGITS))
        .collect();
    let code = (0..).find(|code| !near.contains(code)).unwrap();
    format!("{code:06}")
}

/// Enrols `person`, a demo account with its password: what the enrolment
/// showed, the app that read it, and the recovery codes given.
pub async fn enrol(person: &Client<'_>) -> (Value, App, Vec<String>) {
    let started = person
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let shown = started.json();
    let secret = read_base32(shown["secret"].as_str().unwrap()).unwrap();
    // The step before now: the next code is the current one.
    let mut app = App {
        secret,
        step: step_at(Utc::now().timestamp()) - 1,
    };
    let confirmed = person
        .post(
            "/account/second-factor/confirmation",
            json!({ "code": app.next_code() }),
        )
        .await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.text());
    let codes = confirmed.json()["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|code| code.as_str().unwrap().to_string())
        .collect();
    (shown, app, codes)
}

/// The challenge a sign-in answered with, from its cookie.
pub fn challenge_of(response: &TestResponse) -> String {
    response
        .headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|cookie| cookie.strip_prefix("mimorya_challenge="))
        .and_then(|rest| rest.split(';').next())
        .filter(|token| !token.is_empty())
        .expect("challenge cookie")
        .to_string()
}

/// Whether the response opened a session.
pub fn opens_a_session(response: &TestResponse) -> bool {
    response
        .headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|cookie| cookie.starts_with("mimorya_session=") && !cookie.contains("Max-Age=0"))
}

/// Signs in with a password the second factor is asked after: the
/// challenge.
pub async fn password_step(api: &TestApi, email: &str, password: &str) -> String {
    let response = api.try_login(email, password).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    assert_eq!(response.json()["next"], "second_factor");
    assert!(!opens_a_session(&response));
    challenge_of(&response)
}

/// Completes the challenge `challenge` with `code`, as the browser that
/// holds its cookie.
pub async fn second_step(api: &TestApi, challenge: &str, code: &str) -> TestResponse {
    let request = Request::post("/auth/second-factor")
        .header("x-mimorya-request", "1")
        .header("origin", WEB_ORIGIN)
        .header("content-type", "application/json")
        .header("cookie", format!("mimorya_challenge={challenge}"))
        .body(Body::from(json!({ "code": code }).to_string()))
        .unwrap();
    api.send(request).await
}

/// Sends `body` to the enrolment route `path` of a sign-in (ADR-201), as
/// the browser holding its challenge.
pub async fn enrolment_step(
    api: &TestApi,
    challenge: &str,
    path: &str,
    body: Value,
) -> TestResponse {
    let request = Request::post(path)
        .header("x-mimorya-request", "1")
        .header("origin", WEB_ORIGIN)
        .header("content-type", "application/json")
        .header("cookie", format!("mimorya_challenge={challenge}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    api.send(request).await
}

/// The second factor of `user_id` as the database keeps it: whether it is
/// on, and the codes refused in a row.
pub async fn kept_factor(user_id: Uuid) -> Option<(bool, i32)> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let kept = sqlx::query_as(
        "select enabled_at is not null, failed_attempts from second_factors where user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&owner)
    .await
    .unwrap();
    owner.close().await;
    kept
}

/// The audit entries `action` traced for `user_id`: outcome, reason and
/// metadata, oldest first.
pub async fn traced(
    api: &TestApi,
    tenant_id: Uuid,
    user_id: Uuid,
    action: &str,
) -> Vec<(String, Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let rows = sqlx::query_as(
        "select outcome, reason, metadata from audit_log \
         where actor_user_id = $1 and action = $2 order by id",
    )
    .bind(user_id)
    .bind(action)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}
