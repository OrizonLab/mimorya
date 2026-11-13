//! When the accounts linked at the standard mail go (ADR-205): never
//! followed to another host (a port or a security changed keeps them, a
//! host changed takes them all), with the person's account disabled, with
//! the connector; and the tries a person makes, bounded, as is linking
//! anything without a master key, or at a mail the company disabled.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::{activate, create_account};
use support::mail_servers::{MailServers, PASSWORD, USERNAME, declare, stored};
use support::refusals::latest_entry;
use support::{Client, TestApi};
use uuid::Uuid;

const MAIL: &str = "/account/mail";

async fn link(person: &Client<'_>, username: &str, password: &str) -> StatusCode {
    let linked = person
        .put(MAIL, json!({ "username": username, "password": password }))
        .await;
    linked.status
}

async fn change(admin: &Client<'_>, connector: Uuid, body: Value) {
    let changed = admin
        .patch(&format!("/admin/connectors/{connector}"), body)
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_never_follows_the_mail_to_another_host() {
    let servers = MailServers::for_paul().await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    assert_eq!(link(&paul, USERNAME, PASSWORD).await, StatusCode::OK);

    // The same hosts, another security and the agenda's absence kept: kept.
    let port = servers.smtp.address.port();
    let smtp = json!({ "host": "127.0.0.1", "port": port, "security": "starttls" });
    change(
        &paul,
        connector,
        json!({ "smtp": smtp, "writeTools": true }),
    )
    .await;
    assert!(stored(&api.app, user::PAUL).await.is_some());

    // Another host, even one naming the same server: let go, and audited.
    let since = latest_entry(&api).await;
    let imap = json!({ "host": "localhost", "port": servers.imap.address.port() });
    change(&paul, connector, json!({ "imap": imap })).await;
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let metadata: Value = sqlx::query_scalar(
        "select metadata from audit_log where id > $1 and action = 'connector.updated'",
    )
    .bind(since)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(metadata["mailAccountsRemoved"], 1, "{metadata}");
    // Linked anew where the mail now is.
    assert_eq!(link(&paul, USERNAME, PASSWORD).await, StatusCode::OK);
    let hosts = paul.get(MAIL).await.json()["mail"]["hosts"].clone();
    assert_eq!(hosts, json!(["localhost", "127.0.0.1"]));

    // An agenda added is another host too.
    let agenda = json!({ "caldav": "https://127.0.0.1/dav/" });
    change(&paul, connector, agenda).await;
    assert_eq!(stored(&api.app, user::PAUL).await, None);

    // With the connector, its accounts.
    assert_eq!(link(&paul, USERNAME, PASSWORD).await, StatusCode::OK);
    let removed = paul.delete(&format!("/admin/connectors/{connector}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(stored(&api.app, user::PAUL).await, None);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_person_loses_their_account() {
    let address = "sophie.martin@neo-maintenance.example";
    let servers = MailServers::start(vec![(address.into(), PASSWORD.into())]).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    let mut created = create_account(&paul, "sophie.martin").await;
    let sophie = activate(&api, &mut created).await;
    assert_eq!(link(&sophie, address, PASSWORD).await, StatusCode::OK);
    assert!(stored(&api.app, created.id).await.is_some());

    let disabled = paul
        .patch(
            &format!("/admin/users/{}", created.id),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(stored(&api.app, created.id).await, None);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn tries_are_bounded_and_nothing_is_tried_without_a_master_key() {
    let servers = MailServers::for_paul().await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    // Ten tries an hour, accepted or not; the eleventh never reaches the
    // servers.
    for _ in 0..9 {
        assert_eq!(
            link(&paul, USERNAME, "Mauvais").await,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(link(&paul, USERNAME, PASSWORD).await, StatusCode::OK);
    let tried = servers.imap.heard.lines().len();
    assert_eq!(
        link(&paul, USERNAME, PASSWORD).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(servers.imap.heard.lines().len(), tried);
    // A malformed account is refused before it counts.
    let control = link(&paul, "paul\u{7}", PASSWORD).await;
    assert_eq!(control, StatusCode::BAD_REQUEST);

    // Without a master key, nothing could be kept: refused before any
    // server hears of it.
    let elsewhere = MailServers::for_paul().await;
    let unkeyed = TestApi::start_with(&[("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true")]).await;
    let paul = unkeyed.sign_in(user::PAUL).await;
    declare(&paul, &elsewhere).await;
    let refused = paul
        .put(MAIL, json!({ "username": USERNAME, "password": PASSWORD }))
        .await;
    assert_eq!(
        refused.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        refused.text()
    );
    assert!(elsewhere.imap.heard.lines().is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_mail_takes_no_account_and_shows_the_one_kept() {
    let servers = MailServers::for_paul().await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    assert_eq!(link(&paul, USERNAME, PASSWORD).await, StatusCode::OK);
    change(&paul, connector, json!({ "enabled": false })).await;

    // Whoever linked nothing sees no mail; Paul sees his, unavailable.
    let lucas = api.sign_in(user::LUCAS).await;
    assert_eq!(lucas.get(MAIL).await.json()["mail"], Value::Null);
    let seen = paul.get(MAIL).await.json()["mail"].clone();
    assert_eq!(seen["available"], false, "{seen}");
    assert_eq!(seen["linked"]["username"], USERNAME);
    // Linking again is refused before any server hears of it.
    let tried = servers.imap.heard.lines().len();
    let refused = paul
        .put(MAIL, json!({ "username": USERNAME, "password": PASSWORD }))
        .await;
    assert_eq!(
        refused.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        refused.text()
    );
    assert_eq!(
        refused.error_message(),
        "La messagerie standard de l’entreprise est désactivée"
    );
    assert_eq!(servers.imap.heard.lines().len(), tried);
}
