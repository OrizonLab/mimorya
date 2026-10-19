//! Invitation e-mails (ADR-027): with a relay, the link goes to the invited
//! person and is not shown to the administrator; when sending fails, the
//! administrator gets the link and the reason; a real SMTP exchange carries
//! the message.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::TestApi;
use support::accounts::{INVITED_PASSWORD, accept, invite_account};
use support::mail::{link_in, serve_smtp, token_of};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_link_goes_by_e_mail_when_a_relay_is_configured() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "zoe.leroy").await;
    assert_eq!(invitation["emailed"], true);
    assert_eq!(invitation["link"], json!(null));
    assert_eq!(invitation["problem"], json!(null));
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, account.email);
    assert_eq!(sent[0].to_name, "Nouveau Collègue");
    assert_eq!(
        sent[0].subject,
        "Néo Maintenance (démo) vous invite sur Mimorya"
    );
    assert!(sent[0].text.contains("Paul"), "{}", sent[0].text);
    let accepted = accept(&api, &link_in(&sent[0]), INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_administrator_gets_the_link_when_sending_fails() {
    let api = TestApi::start_with_mailer().await;
    api.mail.fail();
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "remi.blanc").await;
    assert_eq!(invitation["emailed"], false);
    let problem = invitation["problem"].as_str().unwrap();
    assert!(
        problem.starts_with("L’e-mail n’a pas pu être envoyé"),
        "{problem}"
    );
    let link = invitation["link"].as_str().unwrap();
    let accepted = accept(&api, link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let actions: Vec<String> =
        sqlx::query_scalar("select action from audit_log where resource_id = $1 order by id")
            .bind(account.id.to_string())
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert!(actions.contains(&"user.invitation_not_sent".to_string()));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_real_smtp_relay_carries_the_invitation() {
    let (relay, received) = serve_smtp().await;
    let api = TestApi::start_with(&[
        ("SMTP_URL", &relay),
        ("MAIL_FROM", "Mimorya <no-reply@neo-maintenance.example>"),
    ])
    .await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "ines.morel").await;
    assert_eq!(invitation["emailed"], true, "{invitation}");
    let messages = received.lock().unwrap().clone();
    assert_eq!(messages.len(), 1);
    // Quoted-printable may fold long lines.
    let message = messages[0].replace("=\n", "");
    assert!(
        message.contains(&format!("<{}>", account.email)),
        "{message}"
    );
    assert!(
        message.contains("From: Mimorya <no-reply@neo-maintenance.example>"),
        "{message}"
    );
    let link = message
        .lines()
        .find(|line| line.contains("/invitation#"))
        .unwrap_or_else(|| panic!("no link in {message}"));
    let token = token_of(link.trim());
    let accepted = api
        .anonymous()
        .post(
            "/auth/invitation/accept",
            json!({ "token": token, "password": INVITED_PASSWORD }),
        )
        .await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
}
