//! A company's standard mail, declared by its connector administrators
//! (ADR-205): its IMAP and SMTP servers and its CalDAV agenda checked
//! against the network policy, no secret held, one per company, the
//! declaration audited; its servers changed one by one, never an address
//! nor a token; another company never reaching it.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept};
use support::refusals::latest_entry;
use support::second_factor::KEYED;
use support::sso::audit_text_since;
use support::{Client, TestApi, TestResponse, companies};

const DECLARE: &str = "/admin/connectors/mail-server";

fn declaration() -> Value {
    json!({
        "name": "Messagerie de l’entreprise",
        "imap": { "host": "imap.neo-maintenance.example", "port": 993 },
        "smtp": { "host": "smtp.neo-maintenance.example", "port": 587, "security": "starttls" },
    })
}

async fn mail_view(admin: &Client<'_>) -> Value {
    let listed = admin.get("/admin/connectors").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    let body = listed.json();
    let found = body["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "mail_server")
        .cloned();
    found.unwrap_or(Value::Null)
}

async fn declared(admin: &Client<'_>, body: Value) -> String {
    let created = admin.post(DECLARE, body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_string()
}

fn refused_as(response: TestResponse, status: StatusCode, said: &str) {
    assert_eq!(response.status, status, "{}", response.text());
    assert!(
        response.error_message().contains(said),
        "{said}: {}",
        response.text()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_declares_its_mail_servers_once() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let types = paul.get("/admin/connectors").await.json()["types"].clone();
    let offered = types.as_array().unwrap().iter();
    assert!(
        offered.into_iter().any(|t| t["type"] == "mail_server"),
        "{types}"
    );

    let refusals = [
        (("imap", "host"), json!("10.0.0.8"), "Serveur IMAP"),
        (("imap", "host"), json!("imap..example"), "Serveur IMAP"),
        (("smtp", "port"), json!(0), "Serveur SMTP : Port"),
    ];
    for ((server, field), value, said) in refusals {
        let mut body = declaration();
        body[server][field] = value;
        refused_as(
            paul.post(DECLARE, body).await,
            StatusCode::BAD_REQUEST,
            said,
        );
    }
    for caldav in [
        "http://dav.example.com/",
        "https://jean:secret@dav.example.com/",
    ] {
        let mut body = declaration();
        body["caldav"] = json!(caldav);
        let refused = paul.post(DECLARE, body).await;
        refused_as(refused, StatusCode::BAD_REQUEST, "Agenda CalDAV");
    }
    // Nothing but servers: no password, no unknown security.
    let mut password = declaration();
    password["password"] = json!("secret");
    let strict = paul.post(DECLARE, password).await;
    assert_eq!(strict.status, StatusCode::BAD_REQUEST, "{}", strict.text());
    let mut clear = declaration();
    clear["smtp"]["security"] = json!("none");
    let strict = paul.post(DECLARE, clear).await;
    assert_eq!(strict.status, StatusCode::BAD_REQUEST, "{}", strict.text());
    // Only who manages the connectors.
    let claire = api.sign_in(user::CLAIRE).await;
    let forbidden = claire.post(DECLARE, declaration()).await;
    assert_eq!(
        forbidden.status,
        StatusCode::FORBIDDEN,
        "{}",
        forbidden.text()
    );

    let since = latest_entry(&api).await;
    let mut body = declaration();
    body["caldav"] = json!("https://dav.neo-maintenance.example/calendars/");
    let id = declared(&paul, body).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let config: Value =
        sqlx::query_scalar("select config from connector_instances where id = $1::uuid")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    let servers = json!({
        "imap": { "host": "imap.neo-maintenance.example", "port": 993 },
        "smtp": { "host": "smtp.neo-maintenance.example", "port": 587, "security": "starttls" },
        "caldav": "https://dav.neo-maintenance.example/calendars/",
        "writeTools": false,
    });
    assert_eq!(config, servers);
    let view = mail_view(&paul).await;
    assert_eq!(view["mailServer"], servers, "{view}");
    assert_eq!(view["googleWorkspace"], Value::Null);
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(audited.contains("mail_server"), "{audited}");
    assert!(
        audited.contains("imap.neo-maintenance.example"),
        "{audited}"
    );

    // One per company, whatever its name.
    let mut other = declaration();
    other["name"] = json!("Autre messagerie");
    refused_as(
        paul.post(DECLARE, other).await,
        StatusCode::CONFLICT,
        "déjà déclarée",
    );

    let since = latest_entry(&api).await;
    let removed = paul.delete(&format!("/admin/connectors/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(mail_view(&paul).await, Value::Null);
    // The deletion keeps which servers went.
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(
        audited.contains("smtp.neo-maintenance.example"),
        "{audited}"
    );
    // Declared again once gone.
    let again = declared(&paul, declaration()).await;
    let removed = paul.delete(&format!("/admin/connectors/{again}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn its_servers_change_one_by_one_and_it_holds_no_token() {
    // A master key, so that a token would be kept anywhere else.
    let api = TestApi::start_with(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declared(&paul, declaration()).await;
    let path = format!("/admin/connectors/{id}");

    let refusals = [
        (json!({ "token": "jeton-quelconque-1234" }), "pas de jeton"),
        (json!({ "token": null }), "pas de jeton"),
        (
            json!({ "url": "https://mail.example.com/" }),
            "pas d’adresse unique",
        ),
        (json!({ "tokenHeader": "X-Api-Key" }), "Seule une API REST"),
        (
            json!({ "toolPolicies": { "mail_delete_all": { "enabled": false } } }),
            "Outil inconnu de ce connecteur",
        ),
        (
            json!({ "imap": { "host": "127.0.0.1", "port": 993 } }),
            "Serveur IMAP",
        ),
        (
            json!({ "caldav": "http://dav.example.com/" }),
            "Agenda CalDAV",
        ),
    ];
    for (body, said) in refusals {
        refused_as(paul.patch(&path, body).await, StatusCode::BAD_REQUEST, said);
    }

    // Started once: what is known of its servers.
    let tested = paul.post(&format!("{path}/test"), json!({})).await;
    assert_eq!(tested.json()["ok"], true, "{}", tested.text());
    assert_eq!(mail_view(&paul).await["status"], "ok");

    let since = latest_entry(&api).await;
    let changed = paul
        .patch(
            &path,
            json!({
                "smtp": { "host": "smtp.neo-maintenance.example", "port": 465, "security": "tls" },
                "caldav": "https://dav.neo-maintenance.example/",
                "writeTools": true,
                "toolPolicies": { "mail_send": { "permissions": ["planning:read"] } },
            }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let view = mail_view(&paul).await;
    assert_eq!(
        view["mailServer"],
        json!({
            "imap": { "host": "imap.neo-maintenance.example", "port": 993 },
            "smtp": { "host": "smtp.neo-maintenance.example", "port": 465, "security": "tls" },
            "caldav": "https://dav.neo-maintenance.example/",
            "writeTools": true,
        }),
        "{view}"
    );
    // What was known of its servers no longer holds.
    assert_eq!(view["status"], "untested", "{view}");
    // As any tool of a connector the company adds: `connector:use` too.
    assert_eq!(
        view["toolPolicies"]["mail_send"]["permissions"],
        json!(["connector:use", "planning:read"])
    );
    // The audit keeps each server changed, before and after.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let metadata: Value = sqlx::query_scalar(
        "select metadata from audit_log where id > $1 and action = 'connector.updated'",
    )
    .bind(since)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        metadata["mailServers"],
        json!({
            "smtp": {
                "before": { "host": "smtp.neo-maintenance.example", "port": 587, "security": "starttls" },
                "after": { "host": "smtp.neo-maintenance.example", "port": 465, "security": "tls" },
            },
            "caldav": { "before": null, "after": "https://dav.neo-maintenance.example/" },
        })
    );
    // Its agenda removed.
    let removed = paul.patch(&path, json!({ "caldav": null })).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(mail_view(&paul).await["mailServer"]["caldav"], Value::Null);

    // No other connector has servers to change.
    let rest = paul
        .post(
            "/admin/connectors/rest",
            json!({
                "name": "CRM",
                "prefix": "crm",
                "url": "https://api.example.com/v1",
                "operations": [{
                    "name": "list_tickets",
                    "title": "Lister les tickets",
                    "description": "Renvoie les tickets.",
                    "method": "GET",
                    "path": "/tickets",
                }],
            }),
        )
        .await;
    assert_eq!(rest.status, StatusCode::CREATED, "{}", rest.text());
    let rest_path = format!("/admin/connectors/{}", rest.json()["id"].as_str().unwrap());
    let imap = json!({ "imap": { "host": "imap.example.com", "port": 993 } });
    refused_as(
        paul.patch(&rest_path, imap).await,
        StatusCode::BAD_REQUEST,
        "Seule la messagerie standard",
    );
    for gone in [&rest_path, &path] {
        let removed = paul.delete(gone).await;
        assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_never_reaches_it_and_declares_its_own() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declared(&paul, declaration()).await;
    let path = format!("/admin/connectors/{id}");

    let address = "jeanne@acme-messagerie.example";
    let company = companies::create(&api, "acme-messagerie", address).await;
    let accepted = accept(&api, &company.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status.as_u16(), 200, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    // Neither listed, changed, tested nor deleted: answered as absent.
    assert_eq!(mail_view(&jeanne).await, Value::Null);
    let smtp = json!({ "smtp": { "host": "smtp.acme.example", "port": 465, "security": "tls" } });
    let changed = jeanne.patch(&path, smtp).await;
    assert_eq!(changed.status, StatusCode::NOT_FOUND, "{}", changed.text());
    let tested = jeanne.post(&format!("{path}/test"), json!({})).await;
    assert_eq!(tested.status, StatusCode::NOT_FOUND, "{}", tested.text());
    let deleted = jeanne.delete(&path).await;
    assert_eq!(deleted.status, StatusCode::NOT_FOUND, "{}", deleted.text());

    // One per company: its own beside Néo Maintenance's.
    let mut own = declaration();
    own["imap"]["host"] = json!("imap.acme.example");
    let acme = declared(&jeanne, own).await;
    assert_eq!(
        mail_view(&jeanne).await["mailServer"]["imap"]["host"],
        "imap.acme.example"
    );
    assert_eq!(
        mail_view(&paul).await["mailServer"]["imap"]["host"],
        "imap.neo-maintenance.example"
    );
    for (admin, gone) in [(&jeanne, acme), (&paul, id)] {
        let removed = admin.delete(&format!("/admin/connectors/{gone}")).await;
        assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    }
}
