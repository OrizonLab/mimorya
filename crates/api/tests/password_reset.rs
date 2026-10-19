//! Password reset links (ADR-028): an active account asks for a one-time
//! link, e-mailed to it, chooses a new password there and every former
//! session ends; asking gets the same answer whatever the address, and only
//! an active account gets a link; the token reaches neither the audit nor a
//! second use.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::invite_account;
use support::mcp::capture_logs;
use support::resets::{RESET_PASSWORD, ask_reset, complete_reset, inspect_reset, new_reset_token};
use support::{PASSWORD, TestApi, email, owner_url};

const UNUSABLE: &str = "Ce lien de réinitialisation n’est plus valable : demandez-en un nouveau \
     depuis la page de connexion";

async fn audited(api: &TestApi, user_id: uuid::Uuid) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, metadata from audit_log where resource_id = $1 \
         and action like 'user.password_reset%' order by id",
    )
    .bind(user_id.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_resets_the_password_once_and_ends_every_session() {
    let api = TestApi::start_with_mailer().await;
    let address = email(user::LUCAS);
    let former = api.sign_in(user::LUCAS).await;

    // Spaces and capitals are those of a typed address.
    let asked = ask_reset(&api, &format!("  {}  ", address.to_uppercase())).await;
    assert_eq!(asked.status, StatusCode::ACCEPTED, "{}", asked.text());
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, address);
    assert_eq!(sent[0].to_name, "Lucas Petit");
    assert_eq!(
        sent[0].subject,
        "Réinitialisation de votre mot de passe Mimorya"
    );
    assert!(sent[0].text.contains("Néo Maintenance (démo)"));
    let token = support::resets::reset_token_in(&sent[0]);

    let shown = inspect_reset(&api, &token).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    assert_eq!(shown.json()["email"], address);
    let expires: DateTime<Utc> = shown.json()["expiresAt"].as_str().unwrap().parse().unwrap();
    let validity = expires - Utc::now();
    assert!(validity > Duration::minutes(59) && validity <= Duration::hours(1));
    // The former password still works until the new one is chosen.
    api.sign_in_with(&address, PASSWORD).await;

    let short = complete_reset(&api, &token, "court-11car").await;
    assert_eq!(short.status, StatusCode::BAD_REQUEST);
    let reset = complete_reset(&api, &token, RESET_PASSWORD).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    let current = api.session_from(&reset);
    let me = current.get("/auth/me").await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["user"]["mustChangePassword"], false);
    assert_eq!(
        former.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
    let refused = api.try_login(&address, PASSWORD).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    api.sign_in_with(&address, RESET_PASSWORD).await;

    // Once only.
    for used in [
        inspect_reset(&api, &token).await,
        complete_reset(&api, &token, "encore-une-autre-phrase").await,
    ] {
        assert_eq!(used.status, StatusCode::NOT_FOUND);
        assert_eq!(used.error_message(), UNUSABLE);
    }
    let audited = audited(&api, user::LUCAS).await;
    let actions: Vec<&str> = audited.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        actions,
        [
            "user.password_reset_requested",
            "user.password_reset_completed"
        ]
    );
    assert!(audited.iter().all(|(_, m)| !m.to_string().contains(&token)));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn asking_reveals_nothing_and_links_only_active_accounts() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (invited, _) = invite_account(&paul, "reinitialisation.invite").await;
    let disabled = paul
        .patch(
            &format!("/admin/users/{}", user::ISABELLE),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    let invitations = api.mail.sent().len();

    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let suspend = |status: &'static str| {
        sqlx::query("update tenants set status = $1 where id = $2")
            .bind(status)
            .bind(tenant::ID)
            .execute(&owner)
    };
    suspend("suspended").await.unwrap();
    let suspended = ask_reset(&api, &email(user::EMMA)).await;
    suspend("active").await.unwrap();
    let answers = [
        suspended,
        ask_reset(&api, "personne@nulle-part.example").await,
        ask_reset(&api, &invited.email).await,
        ask_reset(&api, &email(user::ISABELLE)).await,
    ];
    for answer in &answers {
        assert_eq!(answer.status, StatusCode::ACCEPTED);
        assert_eq!(answer.json(), json!({ "ok": true }));
    }
    assert_eq!(api.mail.sent().len(), invitations);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let stored: i64 =
        sqlx::query_scalar("select count(*) from password_resets where user_id = any($1)")
            .bind([user::EMMA, user::ISABELLE, invited.id])
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(stored, 0);
    drop(tx);

    // The same answer for an active account, which gets its link.
    let token = new_reset_token(&api, &email(user::EMMA)).await;
    assert_eq!(api.mail.sent().len(), invitations + 1);
    assert_eq!(inspect_reset(&api, &token).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_that_could_not_be_sent_is_audited_but_not_logged() {
    let api = TestApi::start_with_mailer().await;
    api.mail.fail();
    let logs = capture_logs();
    let asked = ask_reset(&api, &email(user::HUGO)).await;
    let logged = logs.text();
    drop(logs);
    assert_eq!(asked.status, StatusCode::ACCEPTED);
    let actions: Vec<String> = audited(&api, user::HUGO)
        .await
        .into_iter()
        .map(|(action, _)| action)
        .collect();
    assert_eq!(
        actions,
        [
            "user.password_reset_requested",
            "user.password_reset_not_sent"
        ]
    );
    assert!(
        logged.contains("Password reset e-mail not sent"),
        "{logged}"
    );
    assert!(!logged.contains(&email(user::HUGO)), "{logged}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_relay_no_link_can_be_asked() {
    let api = TestApi::start().await;
    let refused = api
        .anonymous()
        .post(
            "/auth/password-reset",
            json!({ "email": email(user::HUGO) }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        refused.error_message(),
        "La réinitialisation par e-mail n’est pas disponible : demandez à votre administrateur \
         de réinitialiser votre mot de passe"
    );
}
