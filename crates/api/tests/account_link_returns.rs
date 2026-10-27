//! A link's return when its company's application changed meanwhile
//! (ADR-116): disabled or deleted while the person was at Microsoft, it
//! links nothing and redeems no code; and another company's application
//! does not exist from here.

mod support;

use axum::http::StatusCode;
use mimorya_core::account_links::MICROSOFT365;
use mimorya_core::demo::user;
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::json;
use support::account_links::{
    FAILED, LINKS, come_back, insert_microsoft365, kept_refresh_token, link_entries, links,
    microsoft_person, start, try_start,
};
use support::identity_provider::MockIdentityProvider;
use support::refusals::latest_entry;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{TestApi, email};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_links_nothing_once_its_application_is_disabled_or_deleted() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let connector = insert_microsoft365(&api, &idp, true).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    let path = format!("/admin/connectors/{connector}");

    // Disabled while she is at Microsoft: her code is never redeemed.
    let (url, cookie) = start(&claire, connector).await;
    let back = idp.authorize(&url);
    let disabled = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    let since = latest_entry(&api).await;
    assert_eq!(
        location(&come_back(&claire, &back, Some(&cookie)).await),
        FAILED
    );
    assert_eq!(idp.exchanges(), 0);
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
    let refused = link_entries(&api, since).await;
    assert_eq!(
        refused[0].3.as_deref(),
        Some("Connecteur supprimé ou désactivé")
    );

    // Deleted while she is at Microsoft: the same.
    let enabled = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    let (url, cookie) = start(&claire, connector).await;
    let back = idp.authorize(&url);
    let removed = paul.delete(&path).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(
        location(&come_back(&claire, &back, Some(&cookie)).await),
        FAILED
    );
    assert_eq!(idp.exchanges(), 0);
    assert_eq!(links(&claire).await, json!([]));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_companys_application_does_not_exist_from_here() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let ours = insert_microsoft365(&api, &idp, true).await;
    let foreign = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query(
        "insert into connector_instances (id, tenant_id, type, name, config) \
         values ($1, $2, $3, 'Messagerie étrangère', $4)",
    )
    .bind(foreign)
    .bind(OTHER_TENANT_ID)
    .bind(MICROSOFT365)
    .bind(json!({
        "directory": "72f988bf-86f1-41af-91ab-2d7cd011db47",
        "clientId": "mimorya-tests",
        "issuer": idp.issuer,
        "graph": format!("{}/graph/v1.0", idp.issuer),
    }))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let claire = api.sign_in(user::CLAIRE).await;
    let listed = links(&claire).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["connectorId"], json!(ours));
    let started = try_start(&claire, foreign).await;
    assert_eq!(started.status, StatusCode::NOT_FOUND, "{}", started.text());
    let removed = claire.delete(&format!("{LINKS}/{foreign}")).await;
    assert_eq!(removed.status, StatusCode::NOT_FOUND, "{}", removed.text());

    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query("delete from connector_instances where id = $1")
        .bind(foreign)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}
