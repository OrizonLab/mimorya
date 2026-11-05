//! The OAuth authorizations each person gives an MCP server for themself, as
//! the database keeps them (ADR-174): one per connector and person, a new
//! one replacing only that person's; every read and every removal of one
//! person's touches no one else's, nor another connector's; all of a
//! connector's, or all of a person's, taken together; gone with the
//! connector and with the account; out of reach of another company.

mod support;

use chrono::Utc;
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::repositories::connector_person_authorizations::{
    NewPersonAuthorization, count_person_authorizations, get_person_authorization,
    person_access_token, person_authorizations_of, put_person_authorization,
    take_person_authorization, take_person_authorizations_of_connector,
    take_person_authorizations_of_user,
};
use mimorya_db::repositories::connectors::{
    NewConnectorInstance, create_connector_instance, delete_connector_instance,
};
use mimorya_db::repositories::secrets::StoredSecret;
use mimorya_db::repositories::users::{NewUser, create_user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::json;
use uuid::Uuid;

/// A sealed token, told apart by `mark`.
fn sealed(mark: u8) -> StoredSecret {
    StoredSecret {
        ciphertext: vec![mark; 8],
        iv: vec![0; 12],
        auth_tag: vec![0; 16],
        key_version: 1,
    }
}

/// An enabled MCP server of the demo company.
async fn connector(db: &Database, name: &str) -> Uuid {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let config = json!({ "transport": "http", "prefix": name, "url": "https://mcp.example.com" });
    let id = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id: tenant::ID,
            connector_type: "mcp",
            name,
            enabled: true,
            config: &config,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// The person's authorization of the connector, tokens marked `mark`;
/// whether it replaced one.
async fn authorize(db: &Database, connector_id: Uuid, user_id: Uuid, mark: u8) -> bool {
    let (access, refresh) = (sealed(mark), sealed(mark + 100));
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let replaced = put_person_authorization(
        &mut tx,
        NewPersonAuthorization {
            tenant_id: tenant::ID,
            connector_id,
            user_id,
            issuer: "https://auth.example.com",
            client_id: "mimorya",
            resource: "https://mcp.example.com",
            scopes: "files.read",
            access_token: &access,
            access_expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            refresh_token: Some(&refresh),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    replaced
}

/// The mark of the person's access token for the connector, if any.
async fn token_mark(db: &Database, connector_id: Uuid, user_id: Uuid) -> Option<u8> {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let token = person_access_token(&mut tx, connector_id, user_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    token.map(|token| token.sealed.ciphertext[0])
}

/// The connector's version and state, which a person's authorization
/// leaves as they are.
async fn version(db: &Database, connector_id: Uuid) -> (chrono::DateTime<Utc>, String) {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let row = sqlx::query_as("select updated_at, status from connector_instances where id = $1")
        .bind(connector_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    row
}

async fn count(db: &Database, connector_id: Uuid) -> i64 {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let count = count_person_authorizations(&mut tx, connector_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    count
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_person_keeps_their_own_authorization() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let server = connector(&db, "chacun").await;
    let before = version(&db, server).await;
    assert!(!authorize(&db, server, user::MARC, 1).await);
    assert!(!authorize(&db, server, user::PAUL, 2).await);
    assert_eq!(count(&db, server).await, 2);
    assert_eq!(token_mark(&db, server, user::MARC).await, Some(1));
    assert_eq!(token_mark(&db, server, user::PAUL).await, Some(2));
    assert_eq!(token_mark(&db, server, user::LEA).await, None);

    // A new authorization replaces only the person's own.
    assert!(authorize(&db, server, user::MARC, 3).await);
    // No new version of the connector: a person's session follows their own.
    assert_eq!(version(&db, server).await, before);
    assert_eq!(count(&db, server).await, 2);
    assert_eq!(token_mark(&db, server, user::MARC).await, Some(3));
    assert_eq!(token_mark(&db, server, user::PAUL).await, Some(2));

    let mut tx = db.begin(tenant::ID).await.unwrap();
    let marc = get_person_authorization(&mut tx, server, user::MARC)
        .await
        .unwrap()
        .expect("Marc's");
    assert_eq!((marc.user_id, marc.renewable), (user::MARC, true));
    assert_eq!(marc.scopes, "files.read");
    assert!(
        get_person_authorization(&mut tx, server, user::LEA)
            .await
            .unwrap()
            .is_none()
    );
    let listed = person_authorizations_of(&mut tx, user::MARC).await.unwrap();
    assert!(listed.iter().all(|row| row.user_id == user::MARC));
    assert_eq!(
        listed.iter().filter(|r| r.connector_id == server).count(),
        1
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_removal_takes_only_what_it_names() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let (first, second) = (
        connector(&db, "premier").await,
        connector(&db, "second").await,
    );
    for (server, mark) in [(first, 10), (second, 20)] {
        authorize(&db, server, user::MARC, mark).await;
        authorize(&db, server, user::PAUL, mark + 1).await;
        authorize(&db, server, user::LEA, mark + 2).await;
    }

    let before = version(&db, first).await;

    // One person's, on one connector.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let taken = take_person_authorization(&mut tx, first, user::MARC)
        .await
        .unwrap()
        .expect("taken");
    assert_eq!((taken.connector_id, taken.user_id), (first, user::MARC));
    assert_eq!(taken.taken.access.ciphertext[0], 10);
    assert_eq!(taken.taken.refresh.expect("refresh").ciphertext[0], 110);
    assert!(
        take_person_authorization(&mut tx, first, user::MARC)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();
    assert_eq!(token_mark(&db, second, user::MARC).await, Some(20));
    assert_eq!(token_mark(&db, first, user::PAUL).await, Some(11));
    assert_eq!(version(&db, first).await, before, "no new version either");

    // Every one of a person's: no one else's.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let taken = take_person_authorizations_of_user(&mut tx, user::PAUL)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut marks: Vec<_> = taken.iter().map(|t| t.taken.access.ciphertext[0]).collect();
    marks.sort();
    assert!(marks.contains(&11) && marks.contains(&21), "{marks:?}");
    assert!(taken.iter().all(|t| t.user_id == user::PAUL));
    assert_eq!(token_mark(&db, first, user::LEA).await, Some(12));
    assert_eq!(token_mark(&db, second, user::MARC).await, Some(20));

    // Every one of a connector's: no other connector's.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let taken = take_person_authorizations_of_connector(&mut tx, first)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(taken.len(), 1, "Léa's alone was left on it");
    assert_eq!(
        (taken[0].connector_id, taken[0].user_id),
        (first, user::LEA)
    );
    assert_eq!(count(&db, first).await, 0);
    assert_eq!(count(&db, second).await, 2);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_authorization_goes_with_its_connector_and_its_account() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let server = connector(&db, "parti").await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let leaving = create_user(
        &mut tx,
        NewUser {
            tenant_id: tenant::ID,
            email: "depart.oauth@neo-maintenance.example".into(),
            display_name: "Départ OAuth".into(),
            job_title: None,
            clearance: Sensitivity::Internal,
            password_hash: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    authorize(&db, server, leaving, 1).await;
    authorize(&db, server, user::MARC, 2).await;

    // The application never deletes an account: the operator does, as owner.
    let owner = support::connect(&support::urls(|_| {}).owner).await;
    let mut tx = owner.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from users where id = $1")
        .bind(leaving)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(count(&db, server).await, 1);

    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert!(delete_connector_instance(&mut tx, server).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(token_mark(&db, server, user::MARC).await, None);
    assert_eq!(count(&db, server).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_neither_reads_nor_takes_one() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let server = connector(&db, "ailleurs").await;
    authorize(&db, server, user::MARC, 7).await;

    let mut other = db.begin(OTHER_TENANT_ID).await.unwrap();
    assert!(
        person_access_token(&mut other, server, user::MARC)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        get_person_authorization(&mut other, server, user::MARC)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        person_authorizations_of(&mut other, user::MARC)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        count_person_authorizations(&mut other, server)
            .await
            .unwrap(),
        0
    );
    assert!(
        take_person_authorization(&mut other, server, user::MARC)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        take_person_authorizations_of_user(&mut other, user::MARC)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        take_person_authorizations_of_connector(&mut other, server)
            .await
            .unwrap()
            .is_empty()
    );
    // Nor writes one for this company's connector or person.
    let access = sealed(9);
    let written = put_person_authorization(
        &mut other,
        NewPersonAuthorization {
            tenant_id: tenant::ID,
            connector_id: server,
            user_id: user::PAUL,
            issuer: "https://auth.example.com",
            client_id: "mimorya",
            resource: "https://mcp.example.com",
            scopes: "",
            access_token: &access,
            access_expires_at: None,
            refresh_token: None,
        },
    )
    .await;
    assert!(written.is_err(), "the row-level security refuses it");
    drop(other);
    assert_eq!(token_mark(&db, server, user::MARC).await, Some(7));
    assert_eq!(token_mark(&db, server, user::PAUL).await, None);
}
