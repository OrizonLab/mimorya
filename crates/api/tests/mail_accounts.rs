//! The account each person links at their company's standard mail
//! (ADR-205): kept once the company's IMAP and SMTP servers both accepted
//! it, sealed, never shown nor audited; a refused one never kept; removed
//! by its person; out of reach of anyone else, in the company or not, and
//! of whoever may not use connectors.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, tenant, user};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept, activate, assignment_of, create_account};
use support::mail_servers::{MailServers, PASSWORD, USERNAME, declare, stored};
use support::refusals::latest_entry;
use support::sso::audit_text_since;
use support::{Client, TestApi, companies};

const MAIL: &str = "/account/mail";

async fn mail_of(person: &Client<'_>) -> Value {
    let read = person.get(MAIL).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    read.json()["mail"].clone()
}

fn account(username: &str, password: &str) -> Value {
    json!({ "username": username, "password": password })
}

/// The actions audited after `since`, in order.
async fn actions_since(api: &TestApi, since: i64) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let actions = sqlx::query_scalar("select action from audit_log where id > $1 order by id")
        .bind(since)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    actions
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_is_kept_once_both_servers_accept_it() {
    let servers = MailServers::for_paul().await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    // Before any declaration, nothing to link.
    assert_eq!(mail_of(&paul).await, Value::Null);
    let refused = paul.put(MAIL, account(USERNAME, PASSWORD)).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());

    let connector = declare(&paul, &servers).await;
    let mail = mail_of(&paul).await;
    assert_eq!(mail["connectorId"], json!(connector));
    assert_eq!(mail["available"], true);
    assert_eq!(mail["hosts"], json!(["127.0.0.1", "127.0.0.1"]));
    assert_eq!(mail["linked"], Value::Null);

    // A wrong password: refused by the IMAP server, nothing kept, the
    // password nowhere.
    let since = latest_entry(&api).await;
    let wrong = "Mauvais-Secret-42";
    let refused = paul.put(MAIL, account(USERNAME, wrong)).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused
            .error_message()
            .starts_with("Serveur IMAP : Identifiant ou mot de passe refusé"),
        "{}",
        refused.text()
    );
    assert!(!refused.text().contains(wrong));
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    assert_eq!(actions_since(&api, since).await, ["mail_account.refused"]);
    assert!(
        !audit_text_since(&api, tenant::ID, since)
            .await
            .contains(wrong)
    );
    // Nor did the SMTP server hear of it.
    assert!(
        servers.smtp.heard.lines().is_empty(),
        "{:?}",
        servers.smtp.heard.lines()
    );

    // The right one: both servers signed in to, then kept sealed.
    let since = latest_entry(&api).await;
    let linked = paul.put(MAIL, account(USERNAME, PASSWORD)).await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());
    let (username, sealed) = stored(&api.app, user::PAUL).await.unwrap();
    assert_eq!(username, USERNAME);
    let clear = PASSWORD.as_bytes();
    assert!(!sealed.windows(clear.len()).any(|window| window == clear));
    assert!(
        servers
            .smtp
            .heard
            .lines()
            .iter()
            .any(|line| line.contains(" AUTH "))
    );
    assert_eq!(actions_since(&api, since).await, ["mail_account.linked"]);
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(
        !audited.contains(PASSWORD) && !audited.contains(USERNAME),
        "{audited}"
    );
    let mail = mail_of(&paul).await;
    assert_eq!(mail["linked"]["username"], USERNAME);
    assert!(!mail.to_string().contains(PASSWORD));
    // The administration sees how many linked an account, never whose.
    let connectors = paul.get("/admin/connectors").await.json();
    let declared = connectors["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instance| instance["type"] == "mail_server")
        .unwrap()
        .clone();
    assert_eq!(declared["mailServer"]["accounts"], 1);
    assert!(!declared.to_string().contains(USERNAME));
    // The person's export says which account, never its password.
    let export = paul.get("/account/export").await;
    assert_eq!(export.status, StatusCode::OK);
    let exported = export.text();
    assert!(exported.contains(USERNAME), "{exported}");
    assert!(!exported.contains(PASSWORD));

    // Linked again (a password changed at the host): replaced, one kept.
    let since = latest_entry(&api).await;
    let again = paul.put(MAIL, account(USERNAME, PASSWORD)).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    let (_, resealed) = stored(&api.app, user::PAUL).await.unwrap();
    assert_ne!(resealed, sealed);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let kept: i64 = sqlx::query_scalar("select count(*) from mail_accounts")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let said: Value = sqlx::query_scalar(
        "select metadata from audit_log where id > $1 and action = 'mail_account.linked'",
    )
    .bind(since)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(kept, 1);
    // Paul's identifier is not his account's address: said, never written.
    assert_eq!(said, json!({ "replaced": true, "ownAddress": false }));

    // Removed by its person: gone, and said so the second time.
    let since = latest_entry(&api).await;
    let removed = paul.delete(MAIL).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    assert_eq!(mail_of(&paul).await["linked"], Value::Null);
    assert_eq!(actions_since(&api, since).await, ["mail_account.removed"]);
    let again = paul.delete(MAIL).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND, "{}", again.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nobody_reaches_another_s_account() {
    let servers = MailServers::for_paul().await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    let linked = paul.put(MAIL, account(USERNAME, PASSWORD)).await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());

    // Another person of the company sees the mail, never Paul's account,
    // and has none to remove.
    let lucas = api.sign_in(user::LUCAS).await;
    let seen = mail_of(&lucas).await;
    assert_eq!(seen["available"], true);
    assert_eq!(seen["linked"], Value::Null);
    let removed = lucas.delete(MAIL).await;
    assert_eq!(removed.status, StatusCode::NOT_FOUND, "{}", removed.text());
    assert!(stored(&api.app, user::PAUL).await.is_some());

    // Another company's person: no mail to see, none to link at.
    let address = "jeanne@acme-courriel.example";
    let company = companies::create(&api, "acme-courriel", address).await;
    let accepted = accept(&api, &company.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status.as_u16(), 200, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    assert_eq!(mail_of(&jeanne).await, Value::Null);
    let refused = jeanne.put(MAIL, account(USERNAME, PASSWORD)).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    let removed = jeanne.delete(MAIL).await;
    assert_eq!(removed.status, StatusCode::NOT_FOUND, "{}", removed.text());
    assert!(stored(&api.app, user::PAUL).await.is_some());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_may_use_connectors_links_an_account() {
    let servers = MailServers::for_paul().await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    let mut created = create_account(&paul, "sans.messagerie").await;
    let newcomer = activate(&api, &mut created).await;
    assert_eq!(mail_of(&newcomer).await["available"], true);
    // Its base role, and with it `connector:use`, taken away.
    let base = assignment_of(&paul, created.id, role::MEMBER).await;
    let revoked = paul.delete(&format!("/admin/assignments/{base}")).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
    assert_eq!(mail_of(&newcomer).await, Value::Null);
    let refused = newcomer.put(MAIL, account(USERNAME, PASSWORD)).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    // Refused before any server heard of it.
    assert!(
        servers.imap.heard.lines().is_empty(),
        "{:?}",
        servers.imap.heard.lines()
    );
}
