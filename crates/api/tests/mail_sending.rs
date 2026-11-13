//! The standard mail's `mail_send` in a run (ADR-205): offered once the
//! company lets its tools write, run once the person validated it; the
//! e-mail handed to the company's SMTP server with their account, from
//! their linked mailbox, or from their platform address when the
//! identifier they linked is not one. A password the SMTP server refuses
//! removes the account, audited with the server that refused it. The
//! password is nowhere in the journal.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::mail_accounts::MAIL_ACCOUNT_REFUSED;
use mimorya_connectors::testing::{FakeSmtp, SmtpMode};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::Client;
use support::mail_servers::{MailServers, PASSWORD, USERNAME, declare, stored};
use support::mailbox::{told, validated_tool_result};
use support::refusals::latest_entry;
use support::{TestApi, sso::audit_text_since};
use uuid::Uuid;

/// Paul's platform address.
const PAUL: &str = "paul.renaud@neo-maintenance.example";

async fn link(person: &Client<'_>, username: &str) {
    let linked = person
        .put(
            "/account/mail",
            json!({ "username": username, "password": PASSWORD }),
        )
        .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());
}

/// The company lets the standard mail's tools write.
async fn writing(admin: &Client<'_>, connector: Uuid, changes: Value) {
    let mut changes = changes;
    changes["writeTools"] = json!(true);
    let changed = admin
        .patch(&format!("/admin/connectors/{connector}"), changes)
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
}

fn email() -> Value {
    json!({
        "to": ["claire@exemple.fr"],
        "subject": "Planning de novembre",
        "body": "Bonjour,\nLe planning est prêt.",
    })
}

/// What the run journal kept of tool calls, as text.
async fn journal(api: &TestApi) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<Value> = sqlx::query_scalar("select to_jsonb(t) from tool_invocations t")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    serde_json::to_string(&rows).unwrap()
}

/// The servers named by the refusals audited after `since`.
async fn refusing_servers(api: &TestApi, since: i64) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let servers = sqlx::query_scalar(
        "select metadata->>'server' from audit_log where id > $1 and action = $2 order by id",
    )
    .bind(since)
    .bind(MAIL_ACCOUNT_REFUSED)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    servers
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_sends_what_its_person_validated_from_their_address() {
    // An identifier that is no address: the e-mail leaves from Paul's
    // platform address.
    let servers = MailServers::start(vec![("pdurand".into(), PASSWORD.into())]).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    writing(&paul, connector, json!({})).await;
    link(&paul, "pdurand").await;

    let sent = validated_tool_result(&api, &paul, "mail_send", email()).await;
    assert_eq!(sent["envoye"], true, "{sent}");
    assert_eq!(sent["de"], PAUL);
    let delivered = servers.smtp.deliveries.all();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].from, PAUL);
    assert_eq!(delivered[0].to, ["claire@exemple.fr"]);
    let text = &delivered[0].text;
    assert!(text.starts_with(&format!("From: {PAUL}\r\n")), "{text}");
    assert!(!journal(&api).await.contains(PASSWORD));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_the_smtp_server_refuses_is_removed_audited_so() {
    let servers = MailServers::for_paul().await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    link(&paul, USERNAME).await;
    // The same host on another port, whose server knows another password:
    // the account stays (ADR-205), and is refused there.
    let other = vec![(USERNAME.into(), "Autre-2026!".into())];
    let moved = FakeSmtp::start(&servers.authority, SmtpMode::StartTls, other).await;
    let smtp = json!({ "host": "127.0.0.1", "port": moved.address.port(), "security": "starttls" });
    writing(&paul, connector, json!({ "smtp": smtp })).await;
    assert!(stored(&api.app, user::PAUL).await.is_some());

    let since = latest_entry(&api).await;
    let refused = validated_tool_result(&api, &paul, "mail_send", email()).await;
    assert!(
        told(&refused).starts_with("Le serveur SMTP de l’entreprise refuse le compte relié"),
        "{refused}"
    );
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    assert_eq!(refusing_servers(&api, since).await, ["smtp"]);
    let said = audit_text_since(&api, tenant::ID, since).await;
    assert!(!said.contains(PASSWORD), "{said}");
    assert!(moved.deliveries.all().is_empty());
    assert!(servers.smtp.deliveries.all().is_empty());
}
