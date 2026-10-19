//! Reset links sent by an administrator (ADR-029): the account receives a
//! one-time link by e-mail, valid 24 hours, which the administrator never
//! sees; its current password works until the link is used. Only a
//! company-wide account administrator sends one, to another active account,
//! and a link that could not be sent is withdrawn.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::json;
use support::accounts::{create_partial_admin, invite_account};
use support::resets::{RESET_PASSWORD, complete_reset, last_reset_token};
use support::{PASSWORD, TestApi, email};

fn link_url(user_id: uuid::Uuid) -> String {
    format!("/admin/users/{user_id}/password-link")
}

async fn audited(api: &TestApi, user_id: uuid::Uuid) -> Vec<(String, Option<uuid::Uuid>, String)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, actor_user_id, metadata::text from audit_log where resource_id = $1 \
         and action like 'user.password_reset%' order by id",
    )
    .bind(user_id.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_sends_a_reset_link_by_e_mail() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let directory = paul.get("/admin/users").await.json();
    assert_eq!(directory["emailEnabled"], true);
    let address = email(user::LUCAS);
    let lucas = api.sign_in(user::LUCAS).await;

    let sent = paul.post(&link_url(user::LUCAS), json!({})).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    assert_eq!(sent.json().as_object().unwrap().len(), 1);
    let expires: DateTime<Utc> = sent.json()["expiresAt"].as_str().unwrap().parse().unwrap();
    let validity = expires - Utc::now();
    assert!(validity > Duration::hours(23) && validity <= Duration::hours(24));
    let mail = api.mail.sent().pop().unwrap();
    assert_eq!(mail.to, address);
    assert!(
        mail.text.contains("Paul Renaud vous envoie ce lien"),
        "{}",
        mail.text
    );

    // Until the link is used, the account goes on as before.
    assert_eq!(lucas.get("/auth/me").await.status, StatusCode::OK);
    api.sign_in_with(&address, PASSWORD).await;
    let token = last_reset_token(&api, &address);
    let reset = complete_reset(&api, &token, RESET_PASSWORD).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(lucas.get("/auth/me").await.status, StatusCode::UNAUTHORIZED);
    api.sign_in_with(&address, RESET_PASSWORD).await;

    let audited = audited(&api, user::LUCAS).await;
    let entries: Vec<(&str, Option<uuid::Uuid>)> =
        audited.iter().map(|(a, by, _)| (a.as_str(), *by)).collect();
    assert_eq!(
        entries,
        [
            ("user.password_reset_link_sent", Some(user::PAUL)),
            ("user.password_reset_completed", Some(user::LUCAS)),
        ]
    );
    assert!(audited.iter().all(|(_, _, m)| !m.contains(&token)));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_an_account_administrator_sends_one_to_another_active_account() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (invited, _) = invite_account(&paul, "lien.invite").await;
    let disabled = paul
        .patch(
            &format!("/admin/users/{}", user::ISABELLE),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::OK);
    let mails = api.mail.sent().len();

    let refusals = [
        (
            marc.post(&link_url(user::LUCAS), json!({})).await,
            StatusCode::FORBIDDEN,
        ),
        (
            paul.post(&link_url(user::PAUL), json!({})).await,
            StatusCode::FORBIDDEN,
        ),
        (
            paul.post(&link_url(invited.id), json!({})).await,
            StatusCode::CONFLICT,
        ),
        (
            paul.post(&link_url(user::ISABELLE), json!({})).await,
            StatusCode::CONFLICT,
        ),
        (
            paul.post(&link_url(OTHER_USER_ID), json!({})).await,
            StatusCode::NOT_FOUND,
        ),
        (
            paul.post(&link_url(uuid::Uuid::new_v4()), json!({})).await,
            StatusCode::NOT_FOUND,
        ),
    ];
    for (index, (response, expected)) in refusals.iter().enumerate() {
        assert_eq!(
            response.status,
            *expected,
            "refusal {index}: {}",
            response.text()
        );
    }
    assert_eq!(
        refusals[2].0.error_message(),
        "Ce compte n’a pas encore choisi son mot de passe : renvoyez-lui plutôt son invitation"
    );
    assert_eq!(api.mail.sent().len(), mails);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_relay_the_temporary_password_remains() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    assert_eq!(paul.get("/admin/users").await.json()["emailEnabled"], false);
    let refused = paul.post(&link_url(user::LUCAS), json!({})).await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        refused.error_message(),
        "Aucun serveur d’e-mail n’est configuré : donnez plutôt un mot de passe provisoire"
    );
    // Rights come first: a team administrator learns nothing about the relay.
    let partial = create_partial_admin(&api, &paul, "lien.partiel").await;
    let partial = api.sign_in_with(&partial.email, &partial.password).await;
    let denied = partial.post(&link_url(user::LUCAS), json!({})).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_that_could_not_be_sent_is_withdrawn() {
    let api = TestApi::start_with_mailer().await;
    api.mail.fail();
    let paul = api.sign_in(user::PAUL).await;
    let failed = paul.post(&link_url(user::HUGO), json!({})).await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE);
    let problem = failed.error_message();
    assert!(
        problem.starts_with("L’e-mail n’a pas pu être envoyé (")
            && problem.ends_with(") : donnez plutôt un mot de passe provisoire"),
        "{problem}"
    );
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let left: i64 = sqlx::query_scalar("select count(*) from password_resets where user_id = $1")
        .bind(user::HUGO)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    drop(tx);
    assert_eq!(left, 0);
    let actions: Vec<String> = audited(&api, user::HUGO)
        .await
        .into_iter()
        .map(|(action, ..)| action)
        .collect();
    assert_eq!(
        actions,
        [
            "user.password_reset_link_sent",
            "user.password_reset_not_sent"
        ]
    );
    api.sign_in_with(&email(user::HUGO), PASSWORD).await;
}
