//! The standard mail's `mail_calendar_event_create` in a run (ADR-205):
//! offered once the company declared an agenda and lets its tools write,
//! run once the person validated it; the event put into the agenda of the
//! person the run serves with their account, its attendees invited from
//! the address the server knows them by. A password the agenda refuses
//! then removes the account, audited with the server that refused it. The
//! password is nowhere in the journal.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::mail_accounts::MAIL_ACCOUNT_REFUSED;
use mimorya_connectors::testing::{AGENDA_PATH, CalDavBehaviour, FakeCalDav};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::mail_servers::{MailServers, PASSWORD, USERNAME, declare, stored};
use support::mailbox::{told, validated_tool_result};
use support::refusals::latest_entry;
use support::{TestApi, sso::audit_text_since};

const CREATE: &str = "mail_calendar_event_create";

fn event() -> Value {
    json!({
        "subject": "Point chantier",
        "start": "2026-11-24T10:00:00",
        "durationMinutes": 45,
        "attendees": ["claire@exemple.fr"],
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_creates_the_event_its_person_validated() {
    let paul_mail = vec![(USERNAME.into(), PASSWORD.into())];
    let servers = MailServers::with_agenda(paul_mail.clone(), paul_mail, vec![]).await;
    let api = TestApi::start_with_mail_servers(&servers).await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = declare(&paul, &servers).await;
    let writing = paul
        .patch(
            &format!("/admin/connectors/{connector}"),
            json!({ "writeTools": true }),
        )
        .await;
    assert_eq!(writing.status, StatusCode::OK, "{}", writing.text());
    let linked = paul
        .put(
            "/account/mail",
            json!({ "username": USERNAME, "password": PASSWORD }),
        )
        .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());

    let created = validated_tool_result(&api, &paul, CREATE, event()).await;
    assert_eq!(created["cree"], true, "{created}");
    assert_eq!(created["debut"], "2026-11-24T10:00:00+01:00");
    assert_eq!(created["organisateur"], USERNAME);
    let agenda = servers.agenda.as_ref().unwrap();
    let kept = agenda.created.all();
    assert_eq!(kept.len(), 1);
    let id = created["id"].as_str().unwrap();
    assert_eq!(kept[0].path, format!("{AGENDA_PATH}{id}.ics"));
    let text = kept[0].text.replace("\r\n ", "");
    assert!(text.contains("\r\nDTSTART:20261124T090000Z\r\n"), "{text}");
    assert!(text.contains(&format!("\r\nORGANIZER:mailto:{USERNAME}\r\n")));
    assert!(!journal(&api).await.contains(PASSWORD));

    // The same host on another port, whose agenda knows another password:
    // the account stays (ADR-205), the next event removes it, audited with
    // the agenda, and nothing is created.
    let since = latest_entry(&api).await;
    let knows_another = vec![(USERNAME.into(), "Agenda-Autre-1!".into())];
    let behaviour = CalDavBehaviour::Answers;
    let other = FakeCalDav::start(&servers.authority, knows_another, behaviour, vec![]).await;
    let changed = paul
        .patch(
            &format!("/admin/connectors/{connector}"),
            json!({ "caldav": other.base() }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    assert!(stored(&api.app, user::PAUL).await.is_some());
    let refused = validated_tool_result(&api, &paul, CREATE, event()).await;
    assert!(
        told(&refused).starts_with("Le serveur CalDAV de l’entreprise refuse le compte relié"),
        "{refused}"
    );
    assert_eq!(stored(&api.app, user::PAUL).await, None);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let servers_said: Vec<String> = sqlx::query_scalar(
        "select metadata->>'server' from audit_log where id > $1 and action = $2 order by id",
    )
    .bind(since)
    .bind(MAIL_ACCOUNT_REFUSED)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(servers_said, ["caldav"]);
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(!audited.contains(PASSWORD), "{audited}");
    assert!(other.created.all().is_empty());
}
