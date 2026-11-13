//! The standard mail's agenda in the API (ADR-205): where the company
//! declared one, a person's account is kept only once the agenda, after
//! its IMAP and SMTP servers, accepted it and found an agenda of events;
//! a run reads the agenda of the person it serves with that account; a
//! password the agenda refuses removes the account, audited with the
//! server that refused it.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::mail_accounts::MAIL_ACCOUNT_REFUSED;
use mimorya_connectors::testing::{CalDavBehaviour, FakeCalDav};
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::mail_servers::{MailServers, PASSWORD, USERNAME, declare, stored};
use support::mailbox::{told, tool_result};
use support::refusals::latest_entry;
use support::{Client, TestResponse};
use support::{TestApi, sso::audit_text_since};

async fn link(person: &Client<'_>) -> TestResponse {
    person
        .put(
            "/account/mail",
            json!({ "username": USERNAME, "password": PASSWORD }),
        )
        .await
}

/// The servers named by the audit entries of `action` after `since`.
async fn servers_audited(api: &TestApi, since: i64, action: &str) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let servers = sqlx::query_scalar(
        "select metadata->>'server' from audit_log where id > $1 and action = $2 order by id",
    )
    .bind(since)
    .bind(action)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    servers
}

fn meeting() -> Vec<String> {
    vec![
        "BEGIN:VEVENT\r\nUID:chantier@neo\r\nSUMMARY:Réunion de chantier\r\n\
         DTSTART:20261116T080000Z\r\nDTEND:20261116T090000Z\r\nEND:VEVENT\r\n"
            .into(),
    ]
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_is_kept_once_its_agenda_accepts_it_too() {
    // The agenda knows another password: the account is refused there.
    let paul_mail = vec![(USERNAME.into(), PASSWORD.into())];
    let other = vec![(USERNAME.into(), "Agenda-Autre-1!".into())];
    let servers = MailServers::with_agenda(paul_mail.clone(), other, vec![]).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    let mail = paul.get("/account/mail").await.json();
    assert_eq!(
        mail["mail"]["hosts"],
        json!(["127.0.0.1", "127.0.0.1", "127.0.0.1"])
    );

    let since = latest_entry(&api).await;
    let refused = link(&paul).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused
            .error_message()
            .starts_with("Serveur CalDAV : Identifiant ou mot de passe refusé"),
        "{}",
        refused.text()
    );
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    assert_eq!(
        servers_audited(&api, since, "mail_account.refused").await,
        ["caldav"]
    );
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(!audited.contains(PASSWORD), "{audited}");
    let agenda = servers.agenda.as_ref().unwrap();
    assert_eq!(agenda.heard.lines(), ["PROPFIND /dav/"]);

    // An agenda that accepts it: kept, its agenda of events found.
    let servers = MailServers::with_agenda(paul_mail.clone(), paul_mail, vec![]).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    declare(&paul, &servers).await;
    let linked = link(&paul).await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());
    assert!(stored(&api.app, user::PAUL).await.is_some());
    let heard = servers.agenda.as_ref().unwrap().heard.lines();
    assert_eq!(
        heard.last().map(String::as_str),
        Some("PROPFIND /dav/calendars/paul/")
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_reads_its_persons_agenda_and_a_refusal_removes_the_account() {
    let paul_mail = vec![(USERNAME.into(), PASSWORD.into())];
    let servers = MailServers::with_agenda(paul_mail.clone(), paul_mail, meeting()).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    let linked = link(&paul).await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());

    let week = json!({ "from": "2026-11-16", "to": "2026-11-20" });
    let read = tool_result(&api, &paul, "mail_calendar_events", week.clone()).await;
    assert_eq!(read["nombre"], 1, "{read}");
    assert_eq!(read["rendezVous"][0]["objet"], "Réunion de chantier");
    assert_eq!(read["rendezVous"][0]["debut"], "2026-11-16T09:00:00+01:00");

    // The same host on another port, whose agenda knows another password:
    // the account stays (ADR-205), and is refused there.
    let other = vec![(USERNAME.into(), "Agenda-Autre-1!".into())];
    let moved =
        FakeCalDav::start(&servers.authority, other, CalDavBehaviour::Answers, vec![]).await;
    let changed = paul
        .patch(
            &format!("/admin/connectors/{connector}"),
            json!({ "caldav": moved.base() }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    assert!(stored(&api.app, user::PAUL).await.is_some());
    let since = latest_entry(&api).await;
    let refused = tool_result(&api, &paul, "mail_calendar_events", week).await;
    let said = told(&refused);
    assert!(
        said.starts_with("Le serveur CalDAV de l’entreprise refuse le compte relié"),
        "{refused}"
    );
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    assert_eq!(
        servers_audited(&api, since, MAIL_ACCOUNT_REFUSED).await,
        ["caldav"]
    );
    assert!(!refused.to_string().contains(PASSWORD));
}
