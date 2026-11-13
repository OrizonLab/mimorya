//! The standard mail's reading tools in a run (ADR-205): they read the
//! mailbox of the person the run serves, with the account that person
//! linked, opened for the call, on the servers it was checked against;
//! never another person's, never without an account, never in a skill's
//! evaluation. An account the server refuses is removed, audited, and not
//! tried again. Its password is nowhere in the journal.

mod support;

use axum::http::StatusCode;
use chrono::NaiveDate;
use mimorya_agent::testing::{ScriptedStep, call, last_tool_results};
use mimorya_api::services::mail_accounts::{MAIL_ACCOUNT_REFUSED, refused_mail_account};
use mimorya_api::services::skill_evaluations::NO_LINKED_ACCOUNT;
use mimorya_connectors::testing::{
    FakeImap, ImapBehaviour, ImapSetup, Mailboxes, TestFolder, TestMessage,
};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::Client;
use support::mail_servers::{MailServers, PASSWORD, USERNAME, declare, stored};
use support::mailbox::{told, tool_result};
use support::refusals::latest_entry;
use support::skill_evaluations::{forget, launched, play, verdict, write_case};
use support::skill_proposals::SKILLS;
use support::{TestApi, sso::audit_text_since};
use uuid::Uuid;

const MAIL: &str = "/account/mail";

fn inbox() -> Mailboxes {
    let message = |uid: u32, subject: &str, text: &str| TestMessage {
        uid,
        seen: false,
        day: NaiveDate::from_ymd_opt(2026, 11, uid).unwrap(),
        raw: format!(
            "From: Fournisseur <contact@fournisseur.example>\r\nTo: {USERNAME}\r\n\
             Subject: {subject}\r\n\r\n{text}\r\n"
        ),
    };
    Mailboxes {
        folders: vec![TestFolder {
            name: "INBOX".into(),
            sent: false,
            validity: 77,
            messages: vec![
                message(2, "Facture de mars", "Montant : 120 €"),
                message(5, "Planning chantier", "Lundi 8 h"),
            ],
        }],
        literal_plus: true,
        ..Mailboxes::default()
    }
}

async fn link(person: &Client<'_>) {
    let linked = person
        .put(MAIL, json!({ "username": USERNAME, "password": PASSWORD }))
        .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());
}

/// How many times a server was signed in to.
fn logins(imap: &FakeImap) -> usize {
    imap.heard
        .lines()
        .iter()
        .filter(|line| line.starts_with("LOGIN"))
        .count()
}

/// Whether `action` was audited after `since`.
async fn audited(api: &TestApi, since: i64, action: &str) -> bool {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let found: i64 =
        sqlx::query_scalar("select count(*) from audit_log where id > $1 and action = $2")
            .bind(since)
            .bind(action)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    found > 0
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_reads_its_persons_mailbox_with_their_account() {
    let servers = MailServers::holding(vec![(USERNAME.into(), PASSWORD.into())], inbox()).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    link(&paul).await;
    let before = logins(&servers.imap);

    let found = tool_result(&api, &paul, "mail_search", json!({})).await;
    assert_eq!(found["nombre"], 2, "{found}");
    assert_eq!(found["emails"][0]["objet"], "Planning chantier");
    assert_eq!(found["emails"][0]["id"], "i77.5");
    let read = tool_result(&api, &paul, "mail_read", json!({ "id": "i77.2" })).await;
    assert!(read["texte"].as_str().unwrap().contains("120 €"), "{read}");
    assert_eq!(logins(&servers.imap), before + 2);
    assert!(
        servers
            .imap
            .heard
            .lines()
            .iter()
            .all(|line| !line.starts_with("LOGIN") || line.ends_with(USERNAME))
    );
    assert!(!journal(&api).await.contains(PASSWORD));

    // Lucas linked nothing: no server hears of him.
    let lucas = api.sign_in(user::LUCAS).await;
    let none = tool_result(&api, &lucas, "mail_search", json!({})).await;
    assert!(told(&none).contains("n’a relié aucun compte"), "{none}");
    assert_eq!(logins(&servers.imap), before + 2);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_the_server_refuses_is_removed_and_not_tried_again() {
    let servers = MailServers::holding(vec![(USERNAME.into(), PASSWORD.into())], inbox()).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    link(&paul).await;
    // The same host on another port, whose server knows another password:
    // the account stays (ADR-205), and is refused there.
    let other = vec![(USERNAME.into(), "Autre-2026!".into())];
    let setup = ImapSetup::new(ImapBehaviour::Answers);
    let moved = FakeImap::holding(&servers.authority, other, setup, inbox()).await;
    let imap = json!({ "host": "127.0.0.1", "port": moved.address.port() });
    let changed = paul
        .patch(
            &format!("/admin/connectors/{connector}"),
            json!({ "imap": imap }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    assert!(stored(&api.app, user::PAUL).await.is_some());

    let since = latest_entry(&api).await;
    let refused = tool_result(&api, &paul, "mail_search", json!({})).await;
    assert!(
        told(&refused).contains("refuse le compte relié"),
        "{refused}"
    );
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    assert!(audited(&api, since, MAIL_ACCOUNT_REFUSED).await);
    let said = audit_text_since(&api, tenant::ID, since).await;
    assert!(
        !said.contains(PASSWORD) && !said.contains(USERNAME),
        "{said}"
    );
    let again = tool_result(&api, &paul, "mail_search", json!({})).await;
    assert!(told(&again).contains("n’a relié aucun compte"), "{again}");
    assert_eq!(logins(&moved), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_reads_no_ones_mailbox() {
    let servers = MailServers::holding(vec![(USERNAME.into(), PASSWORD.into())], inbox()).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    // Whom it runs for linked theirs: still not read.
    link(&paul).await;
    let before = logins(&servers.imap);
    let created = paul
        .post(
            SKILLS,
            json!({ "name": "courrier-evalue", "description": "Relire le courrier.", "content": "1. Lire." }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let skill: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    write_case(&paul, skill, "Courrier", "Qu’ai-je reçu ?").await;
    api.script(vec![
        ScriptedStep::calls("", vec![call("mail_search", json!({}))]),
        ScriptedStep::text("Rien à signaler."),
        verdict(true),
    ]);
    launched(&paul, skill, 1).await;
    play(&api).await;

    let requests = api.model.as_ref().unwrap().requests();
    let result = last_tool_results(&requests[1]);
    assert!(result[0].content.contains(NO_LINKED_ACCOUNT), "{result:?}");
    assert_eq!(logins(&servers.imap), before);
    forget(&api, skill).await;
    paul.delete(&format!("{SKILLS}/{skill}")).await;
}

/// When Paul's account was linked.
async fn linked_on(api: &TestApi) -> chrono::DateTime<chrono::Utc> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let linked = sqlx::query_scalar("select linked_at from mail_accounts where user_id = $1")
        .bind(user::PAUL)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    linked
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_refusal_never_removes_an_account_linked_again_since() {
    let servers = MailServers::holding(vec![(USERNAME.into(), PASSWORD.into())], inbox()).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    link(&paul).await;
    let refused_one = linked_on(&api).await;
    // Linked again (a new password) while a run was refused the old one.
    link(&paul).await;
    let current = linked_on(&api).await;
    assert_ne!(current, refused_one);
    let since = latest_entry(&api).await;
    let db = &api.app.services.db;
    let removed = refused_mail_account(db, tenant::ID, user::PAUL, refused_one).await;
    assert!(!removed.unwrap());
    assert!(stored(&api.app, user::PAUL).await.is_some());
    assert!(!audited(&api, since, MAIL_ACCOUNT_REFUSED).await);
    // The account refused is the one kept: removed, and audited.
    let removed = refused_mail_account(db, tenant::ID, user::PAUL, current).await;
    assert!(removed.unwrap());
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    assert!(audited(&api, since, MAIL_ACCOUNT_REFUSED).await);
}
