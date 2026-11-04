//! The renewal of an MCP server's OAuth authorization as the database keeps
//! it (ADR-171): claimed once, not again while the claim holds, never for a
//! disabled connector; what a renewal brings, or the authorization's
//! removal, written only on the authorization claimed: not on one given
//! again meanwhile, nor on one renewed since, nor on another connector's.

mod support;

use std::time::Duration;

use chrono::Utc;
use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::repositories::connector_authorizations::{
    NewConnectorAuthorization, put_connector_authorization,
};
use mimorya_db::repositories::connector_renewals::{
    Renewed, claim_connector_renewal, connector_authorizations_due, keep_connector_renewal,
    remove_claimed_authorization,
};
use mimorya_db::repositories::connectors::{NewConnectorInstance, create_connector_instance};
use mimorya_db::repositories::secrets::StoredSecret;
use serde_json::json;
use uuid::Uuid;

const WITHIN: Duration = Duration::from_secs(240);
const HOLD: Duration = Duration::from_secs(60);

/// A sealed token, told apart by `mark`.
fn sealed(mark: u8) -> StoredSecret {
    StoredSecret {
        ciphertext: vec![mark; 8],
        iv: vec![0; 12],
        auth_tag: vec![0; 16],
        key_version: 1,
    }
}

/// An MCP server of the demo company, enabled or not.
async fn connector(db: &Database, name: &str, enabled: bool) -> Uuid {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let config = json!({ "transport": "http", "prefix": name, "url": "https://mcp.example.com" });
    let id = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id: tenant::ID,
            connector_type: "mcp",
            name,
            enabled,
            config: &config,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// Authorizes the connector with tokens marked `mark`, expiring in two
/// minutes: due for renewal.
async fn authorize(db: &Database, connector_id: Uuid, mark: u8) {
    let (access, refresh) = (sealed(mark), sealed(mark + 100));
    let mut tx = db.begin(tenant::ID).await.unwrap();
    put_connector_authorization(
        &mut tx,
        NewConnectorAuthorization {
            tenant_id: tenant::ID,
            connector_id,
            issuer: "https://auth.example.com",
            client_id: "mimorya",
            resource: "https://mcp.example.com",
            scopes: "",
            access_token: &access,
            access_expires_at: Some(Utc::now() + chrono::Duration::minutes(2)),
            refresh_token: Some(&refresh),
            authorized_by_user_id: user::PAUL,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// The connector's kept access token's mark and when it was last renewed.
async fn kept(db: &Database, connector_id: Uuid) -> Option<(u8, chrono::DateTime<Utc>)> {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let row: Option<(Vec<u8>, chrono::DateTime<Utc>)> = sqlx::query_as(
        "select access_ciphertext, refreshed_at from connector_authorizations \
         where connector_id = $1",
    )
    .bind(connector_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    row.map(|(access, at)| (access[0], at))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_renewal_is_claimed_once_and_never_for_a_disabled_connector() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let enabled = connector(&db, "reclame", true).await;
    let disabled = connector(&db, "eteint", false).await;
    authorize(&db, enabled, 1).await;
    authorize(&db, disabled, 2).await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let due = connector_authorizations_due(&mut tx, WITHIN).await.unwrap();
    assert!(due.contains(&enabled) && !due.contains(&disabled));
    let first = claim_connector_renewal(&mut tx, enabled, WITHIN, HOLD).await;
    assert!(first.unwrap().is_some());
    let again = claim_connector_renewal(&mut tx, enabled, WITHIN, HOLD).await;
    assert!(again.unwrap().is_none());
    let off = claim_connector_renewal(&mut tx, disabled, WITHIN, HOLD).await;
    assert!(off.unwrap().is_none());
    // Claimed: no longer due, until its claim is over.
    let due = connector_authorizations_due(&mut tx, WITHIN).await.unwrap();
    assert!(!due.contains(&enabled));
    sqlx::query(
        "update connector_authorizations set renewing_until = now() where connector_id = $1",
    )
    .bind(enabled)
    .execute(&mut *tx)
    .await
    .unwrap();
    let over = claim_connector_renewal(&mut tx, enabled, WITHIN, HOLD).await;
    assert!(over.unwrap().is_some());
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_renewal_writes_only_on_the_authorization_it_claimed() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let (first, second) = (
        connector(&db, "premier", true).await,
        connector(&db, "second", true).await,
    );
    authorize(&db, first, 10).await;
    authorize(&db, second, 20).await;
    let untouched = kept(&db, second).await;
    let renewed = sealed(11);
    let renewal = || Renewed {
        access_token: &renewed,
        access_expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
        refresh_token: None,
        scopes: None,
    };

    // Kept on the one claimed, the other connector's left as it was.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let claimed = claim_connector_renewal(&mut tx, first, WITHIN, HOLD)
        .await
        .unwrap()
        .expect("claimed")
        .key;
    tx.commit().await.unwrap();
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let written = keep_connector_renewal(&mut tx, claimed, renewal()).await;
    assert_eq!(written.unwrap(), Some(first));
    tx.commit().await.unwrap();
    assert_eq!(kept(&db, first).await.map(|(mark, _)| mark), Some(11));
    assert_eq!(kept(&db, second).await, untouched);

    // Renewed since: the claim's key is stale, nothing written or removed.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let stale = keep_connector_renewal(&mut tx, claimed, renewal()).await;
    assert_eq!(stale.unwrap(), None);
    assert!(
        !remove_claimed_authorization(&mut tx, claimed)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!(kept(&db, first).await.map(|(mark, _)| mark), Some(11));

    // Given again while a renewal of the former was under way: the new one
    // is neither overwritten nor removed.
    authorize(&db, first, 12).await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let claimed = claim_connector_renewal(&mut tx, first, WITHIN, HOLD)
        .await
        .unwrap()
        .expect("claimed")
        .key;
    tx.commit().await.unwrap();
    authorize(&db, first, 13).await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert_eq!(
        keep_connector_renewal(&mut tx, claimed, renewal())
            .await
            .unwrap(),
        None
    );
    assert!(
        !remove_claimed_authorization(&mut tx, claimed)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!(kept(&db, first).await.map(|(mark, _)| mark), Some(13));

    // Removed on the one claimed, the other connector's left as it was.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let claimed = claim_connector_renewal(&mut tx, second, WITHIN, HOLD)
        .await
        .unwrap()
        .expect("claimed")
        .key;
    assert!(
        remove_claimed_authorization(&mut tx, claimed)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!(kept(&db, second).await, None);
    assert_eq!(kept(&db, first).await.map(|(mark, _)| mark), Some(13));
}
