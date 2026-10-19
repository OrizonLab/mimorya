//! Password reset links in tests (ADR-028): asking for one (and waiting for
//! the background work it starts), the token of the link e-mailed, and the
//! two steps the link opens.

use mimorya_api::mail::Mail;
use serde_json::json;

use super::{TestApi, TestResponse};

/// The password chosen through a reset link.
pub const RESET_PASSWORD: &str = "phrase-de-passe-retrouvee";

/// Asks for a link for `email`, then waits until it is issued (or not).
pub async fn ask_reset(api: &TestApi, email: &str) -> TestResponse {
    let response = api
        .anonymous()
        .post("/auth/password-reset", json!({ "email": email }))
        .await;
    api.idle().await;
    response
}

/// The token of the reset link quoted in an e-mail's text.
pub fn reset_token_in(mail: &Mail) -> String {
    mail.text
        .lines()
        .find_map(|line| line.split_once("/reset-password#"))
        .map(|(_, token)| token.to_string())
        .unwrap_or_else(|| panic!("no reset link in {}", mail.text))
}

/// The token of the last link e-mailed to `email`.
pub fn last_reset_token(api: &TestApi, email: &str) -> String {
    let sent = api.mail.sent();
    let mail = sent
        .iter()
        .rev()
        .find(|m| m.to == email && m.text.contains("/reset-password#"))
        .unwrap_or_else(|| panic!("no reset link sent to {email}"));
    reset_token_in(mail)
}

/// Asks for a link for `email` and returns its token.
pub async fn new_reset_token(api: &TestApi, email: &str) -> String {
    let asked = ask_reset(api, email).await;
    assert_eq!(asked.status.as_u16(), 202, "{}", asked.text());
    last_reset_token(api, email)
}

pub async fn inspect_reset(api: &TestApi, token: &str) -> TestResponse {
    api.anonymous()
        .post("/auth/password-reset/link", json!({ "token": token }))
        .await
}

pub async fn complete_reset(api: &TestApi, token: &str, password: &str) -> TestResponse {
    api.anonymous()
        .post(
            "/auth/password-reset/complete",
            json!({ "token": token, "password": password }),
        )
        .await
}
