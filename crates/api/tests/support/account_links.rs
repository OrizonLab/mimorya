//! Accounts linked at the company's Microsoft 365 application (ADR-116) or
//! Google Workspace client (ADR-121) in the tests: the application declared
//! at the local provider, a person's visit to it and return, and what the
//! database and the audit keep.

use axum::http::StatusCode;
use mimorya_api::secrets::SecretStore;
use mimorya_api::services::connectors::token_secret_name;
use mimorya_connectors::testing::FakeGoogle;
use mimorya_core::account_links::{GOOGLE_WORKSPACE, MICROSOFT365, sealed_as};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::account_links::get_account_link;
use mimorya_db::repositories::connectors::{NewConnectorInstance, create_connector_instance};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::identity_provider::{
    CLIENT_ID, CLIENT_SECRET, GOOGLE_CLIENT_ID, MockIdentityProvider, Person,
};
use super::sso::set_cookie;
use super::{Client, TestApi, TestResponse};

pub const LINKS: &str = "/account/links";
pub const LINKED: &str = "/account?liaison=ok";
pub const FAILED: &str = "/account?liaison=echec";
pub const OTHER_ACCOUNT: &str = "/account?liaison=autre-compte";
/// The directory the declarations name: its issuer is the local provider's.
pub const DIRECTORY: &str = "72f988bf-86f1-41af-91ab-2d7cd011db47";

/// The company's Microsoft 365 application, its sign-in at the local
/// provider (the administration writes Microsoft's), enabled or not; one
/// a previous test declared goes first, with its links.
pub async fn insert_microsoft365(api: &TestApi, idp: &MockIdentityProvider, enabled: bool) -> Uuid {
    let graph = format!("{}/graph/v1.0", idp.issuer);
    insert_application(api, idp, &graph, enabled, false).await
}

/// The same, enabled, its tools reading this Graph (ADR-117).
pub async fn insert_microsoft365_reading(
    api: &TestApi,
    idp: &MockIdentityProvider,
    graph: &str,
) -> Uuid {
    insert_application(api, idp, graph, true, false).await
}

/// The same, its tools writing too (ADR-119).
pub async fn insert_microsoft365_writing(
    api: &TestApi,
    idp: &MockIdentityProvider,
    graph: &str,
) -> Uuid {
    insert_application(api, idp, graph, true, true).await
}

async fn insert_application(
    api: &TestApi,
    idp: &MockIdentityProvider,
    graph: &str,
    enabled: bool,
    write_tools: bool,
) -> Uuid {
    let config = json!({
        "directory": DIRECTORY,
        "clientId": CLIENT_ID,
        "issuer": idp.issuer,
        "graph": graph,
        "writeTools": write_tools,
    });
    insert_declared(api, MICROSOFT365, "Microsoft 365", enabled, &config).await
}

/// The company's Google Workspace client, its sign-in at the local
/// provider (the administration writes Google's), enabled or not, its tools
/// writing or not; one a previous test declared goes first, with its links
/// (ADR-121).
pub async fn insert_google_workspace(
    api: &TestApi,
    idp: &MockIdentityProvider,
    enabled: bool,
    write_tools: bool,
) -> Uuid {
    let gmail = format!("{}/gmail/v1", idp.issuer);
    let calendar = format!("{}/calendar/v3", idp.issuer);
    insert_google(api, idp, &gmail, &calendar, enabled, write_tools).await
}

/// The same, enabled, its tools reading the test Gmail and Calendar
/// (ADR-122).
pub async fn insert_google_workspace_reading(
    api: &TestApi,
    idp: &MockIdentityProvider,
    google: &FakeGoogle,
) -> Uuid {
    insert_google(api, idp, &google.gmail, &google.calendar, true, false).await
}

/// The same, its tools writing too (ADR-123).
pub async fn insert_google_workspace_writing(
    api: &TestApi,
    idp: &MockIdentityProvider,
    google: &FakeGoogle,
) -> Uuid {
    insert_google(api, idp, &google.gmail, &google.calendar, true, true).await
}

async fn insert_google(
    api: &TestApi,
    idp: &MockIdentityProvider,
    gmail: &str,
    calendar: &str,
    enabled: bool,
    write_tools: bool,
) -> Uuid {
    let config = json!({
        "clientId": GOOGLE_CLIENT_ID,
        "issuer": idp.issuer,
        "gmail": gmail,
        "calendar": calendar,
        "writeTools": write_tools,
    });
    insert_declared(api, GOOGLE_WORKSPACE, "Google Workspace", enabled, &config).await
}

async fn insert_declared(
    api: &TestApi,
    connector_type: &str,
    name: &str,
    enabled: bool,
    config: &Value,
) -> Uuid {
    let services = &api.app.services;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from connector_instances where type = $1")
        .bind(connector_type)
        .execute(&mut *tx)
        .await
        .unwrap();
    let id = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id: tenant::ID,
            connector_type,
            name,
            enabled,
            config,
        },
    )
    .await
    .unwrap();
    SecretStore::new(services.config.secrets.clone())
        .put(&mut tx, tenant::ID, &token_secret_name(id), CLIENT_SECRET)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

/// A directory's account, as Microsoft's token says it: its object ID and
/// its sign-in name.
pub fn microsoft_person(object_id: &str, sign_in_name: &str) -> Person {
    let mut extra = Map::new();
    extra.insert("oid".into(), Value::String(object_id.into()));
    Person {
        subject: format!("sujet-{object_id}"),
        preferred_username: Some(sign_in_name.into()),
        extra,
        ..Person::default()
    }
}

/// A Google account, as Google's token says it: its subject and an
/// address it verified, in this Workspace domain if any (a consumer
/// account has none).
pub fn google_person(subject: &str, address: &str, hosted_domain: Option<&str>) -> Person {
    let mut person = Person::verified(subject, address);
    if let Some(domain) = hosted_domain {
        person
            .extra
            .insert("hd".into(), Value::String(domain.into()));
    }
    person
}

pub async fn try_start(person: &Client<'_>, connector: Uuid) -> TestResponse {
    person
        .post(&format!("{LINKS}/{connector}"), json!({}))
        .await
}

/// The provider's address and the attempt's cookie (`name=value`).
pub async fn start(person: &Client<'_>, connector: Uuid) -> (String, String) {
    let started = try_start(person, connector).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let cookie = set_cookie(&started, "mimorya_link").expect("attempt cookie");
    assert!(cookie.contains("HttpOnly") && cookie.contains("Path=/api/account/links"));
    let pair = cookie.split(';').next().unwrap().to_string();
    let url = started.json()["redirectUrl"].as_str().unwrap().to_string();
    (url, pair)
}

/// The browser's return from the provider, signed in as `person`, with
/// this attempt's cookie, if any.
pub async fn come_back(person: &Client<'_>, path: &str, cookie: Option<&str>) -> TestResponse {
    match cookie {
        Some(cookie) => person.get_with(path, &[("cookie", cookie)]).await,
        None => person.get(path).await,
    }
}

/// From the start to the return, signed in at the provider as it was told.
pub async fn link(
    person: &Client<'_>,
    idp: &MockIdentityProvider,
    connector: Uuid,
) -> TestResponse {
    let (url, cookie) = start(person, connector).await;
    come_back(person, &idp.authorize(&url), Some(&cookie)).await
}

/// The person's links, as listed to them.
pub async fn links(person: &Client<'_>) -> Value {
    let listed = person.get(LINKS).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    listed.json()["connectors"].clone()
}

/// The refresh token the person's link keeps, opened as the server opens it.
pub async fn kept_refresh_token(api: &TestApi, connector: Uuid, user_id: Uuid) -> Option<String> {
    let services = &api.app.services;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let found = get_account_link(&mut tx, connector, user_id).await.unwrap();
    tx.commit().await.unwrap();
    let (_, sealed) = found?;
    let cipher = services.config.secrets.clone().expect("master key");
    Some(
        cipher
            .open(tenant::ID, &sealed_as(connector, user_id), &sealed)
            .expect("sealed for this link"),
    )
}

/// The audit entries about links after `since`: action, outcome, person,
/// reason and metadata.
pub async fn link_entries(
    api: &TestApi,
    since: i64,
) -> Vec<(String, String, Option<Uuid>, Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select action, outcome, actor_user_id, reason, metadata from audit_log \
         where id > $1 and action like 'account.link.%' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}
