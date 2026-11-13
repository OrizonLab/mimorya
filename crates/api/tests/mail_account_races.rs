//! What changes while a person's account is checked at their company's
//! mail servers (ADR-205): their account disabled, or a server moved to
//! another host, nothing is kept. The account is written under the
//! administration lock, which every change of the connector holds: a change
//! in progress is waited for, then seen, never crossed.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use mimorya_connectors::testing::Hold;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::{activate, create_account};
use support::mail_servers::{MailServers, PASSWORD, USERNAME, declare, stored};
use support::refusals::latest_entry;
use support::{Client, TestApi};

const MAIL: &str = "/account/mail";

fn account(username: &str) -> Value {
    json!({ "username": username, "password": PASSWORD })
}

/// Whether an account was linked after `since`.
async fn linked_since(api: &TestApi, since: i64) -> bool {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let linked: i64 = sqlx::query_scalar(
        "select count(*) from audit_log where id > $1 and action = 'mail_account.linked'",
    )
    .bind(since)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    linked > 0
}

/// Moves the company's IMAP server to another host naming the same server.
async fn moved(admin: &Client<'_>, connector: uuid::Uuid, servers: &MailServers) {
    let imap = json!({ "host": "localhost", "port": servers.imap.address.port() });
    let changed = admin
        .patch(
            &format!("/admin/connectors/{connector}"),
            json!({ "imap": imap }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_disabled_during_the_check_keeps_nothing() {
    let address = "course.messagerie@neo-maintenance.example";
    let hold = Arc::new(Hold::default());
    let servers = MailServers::held(vec![(address.into(), PASSWORD.into())], hold.clone()).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    let mut created = create_account(&paul, "course.messagerie").await;
    let leaving = activate(&api, &mut created).await;
    let since = latest_entry(&api).await;
    let disable = async {
        hold.holding().await;
        let disabled = paul
            .patch(
                &format!("/admin/users/{}", created.id),
                json!({ "status": "disabled" }),
            )
            .await;
        assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
        hold.release();
    };
    let (linked, ()) = tokio::join!(leaving.put(MAIL, account(address)), disable);
    assert_eq!(linked.status, StatusCode::FORBIDDEN, "{}", linked.text());
    assert_eq!(stored(&api.app, created.id).await, None);
    assert!(!linked_since(&api, since).await);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_server_moved_during_the_check_keeps_nothing() {
    let hold = Arc::new(Hold::default());
    let servers = MailServers::held(vec![(USERNAME.into(), PASSWORD.into())], hold.clone()).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    let since = latest_entry(&api).await;
    let move_it = async {
        hold.holding().await;
        moved(&paul, connector, &servers).await;
        hold.release();
    };
    let (linked, ()) = tokio::join!(paul.put(MAIL, account(USERNAME)), move_it);
    assert_eq!(linked.status, StatusCode::CONFLICT, "{}", linked.text());
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    assert!(!linked_since(&api, since).await);
}

/// Returns once a transaction waits for an advisory lock.
async fn waiting_for_the_lock(api: &TestApi) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        let waiting: i64 = sqlx::query_scalar(
            "select count(*) from pg_locks where locktype = 'advisory' and not granted",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        if waiting > 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the account was written without waiting for the change"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_waits_for_a_change_in_progress_then_sees_it() {
    let hold = Arc::new(Hold::default());
    let servers = MailServers::held(vec![(USERNAME.into(), PASSWORD.into())], hold.clone()).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    // A change of the connector, as the administration makes it: under its
    // lock, the accounts removed first, the servers changed after.
    let change = async {
        hold.holding().await;
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        tx.lock_administration().await.unwrap();
        sqlx::query("delete from mail_accounts where connector_id = $1")
            .bind(connector)
            .execute(&mut *tx)
            .await
            .unwrap();
        hold.release();
        waiting_for_the_lock(&api).await;
        sqlx::query(
            "update connector_instances \
             set config = jsonb_set(config, '{imap,host}', '\"localhost\"') where id = $1",
        )
        .bind(connector)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
    };
    let (linked, ()) = tokio::join!(paul.put(MAIL, account(USERNAME)), change);
    assert_eq!(linked.status, StatusCode::CONFLICT, "{}", linked.text());
    assert_eq!(stored(&api.app, user::PAUL).await, None);
}
