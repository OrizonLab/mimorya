//! The renewal of an MCP server's OAuth authorization as the database keeps
//! it (ADR-171): claimed once, not again while the claim holds, never for a
//! disabled connector's runs (its test claims it); what a renewal brings,
//! the authorization's removal or why its renewal failed, written only on
//! the authorization claimed: not on one given again meanwhile, nor on one
//! renewed since, nor on another connector's; a failure said until a
//! renewal is kept.

mod support;

use std::time::Duration;

use chrono::Utc;
use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::repositories::connector_authorizations::get_connector_authorization;
use mimorya_db::repositories::connector_authorizations::{
    NewConnectorAuthorization, put_connector_authorization,
};
use mimorya_db::repositories::connector_renewals::{
    ClaimedRenewal, RenewalFor, Renewed, claim_connector_renewal, connector_authorizations_due,
    keep_connector_renewal, record_renewal_failure, remove_claimed_authorization,
};
use mimorya_db::repositories::connectors::{NewConnectorInstance, create_connector_instance};
use mimorya_db::repositories::secrets::StoredSecret;
use serde_json::json;
use uuid::Uuid;

const WITHIN: Duration = Duration::from_secs(240);
const HOLD: Duration = Duration::from_secs(60);
const RUNS: RenewalFor = RenewalFor::Runs;

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
async fn a_renewal_is_claimed_once_and_never_for_a_disabled_connector_s_runs() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let enabled = connector(&db, "reclame", true).await;
    let disabled = connector(&db, "eteint", false).await;
    authorize(&db, enabled, 1).await;
    authorize(&db, disabled, 2).await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let due = connector_authorizations_due(&mut tx, WITHIN).await.unwrap();
    assert!(due.contains(&enabled) && !due.contains(&disabled));
    let first = claim_connector_renewal(&mut tx, enabled, WITHIN, HOLD, RUNS).await;
    assert!(first.unwrap().is_some());
    let again = claim_connector_renewal(&mut tx, enabled, WITHIN, HOLD, RUNS).await;
    assert!(again.unwrap().is_none());
    let off = claim_connector_renewal(&mut tx, disabled, WITHIN, HOLD, RUNS).await;
    assert!(off.unwrap().is_none());
    // Its test starts it all the same: renewed first.
    let tested = claim_connector_renewal(&mut tx, disabled, WITHIN, HOLD, RenewalFor::Test).await;
    assert!(tested.unwrap().is_some());
    // Claimed: no longer due, until its claim is over.
    let due = connector_authorizations_due(&mut tx, WITHIN).await.unwrap();
    assert!(!due.contains(&enabled));
    tx.commit().await.unwrap();
    end_claim(&db, enabled).await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let over = claim_connector_renewal(&mut tx, enabled, WITHIN, HOLD, RUNS).await;
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
    let claimed = claim_connector_renewal(&mut tx, first, WITHIN, HOLD, RUNS)
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
    let claimed = claim_connector_renewal(&mut tx, first, WITHIN, HOLD, RUNS)
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
    let claimed = claim_connector_renewal(&mut tx, second, WITHIN, HOLD, RUNS)
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

/// Why the connector's last renewal failed, as kept.
async fn failure(db: &Database, connector_id: Uuid) -> Option<String> {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let row = get_connector_authorization(&mut tx, connector_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    row.and_then(|row| row.renewal_failure)
}

/// Claims the connector's renewal for runs.
async fn claim(db: &Database, connector_id: Uuid) -> ClaimedRenewal {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let claimed = claim_connector_renewal(&mut tx, connector_id, WITHIN, HOLD, RUNS)
        .await
        .unwrap()
        .expect("claimed")
        .key;
    tx.commit().await.unwrap();
    claimed
}

/// Says on the claimed authorization why its renewal failed.
async fn fail(db: &Database, claimed: ClaimedRenewal, reason: &str) -> bool {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let said = record_renewal_failure(&mut tx, claimed, reason)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    said
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_renewal_failure_is_said_on_the_authorization_claimed_until_one_is_kept() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let (first, second) = (
        connector(&db, "echoue", true).await,
        connector(&db, "voisin", true).await,
    );
    authorize(&db, first, 30).await;
    authorize(&db, second, 40).await;

    // Said on the one claimed, bounded; the other connector's says nothing.
    let claimed = claim(&db, first).await;
    let long = format!("Serveur injoignable {}", "x".repeat(400));
    assert!(fail(&db, claimed, &long).await);
    let said = failure(&db, first).await.expect("said");
    assert_eq!(said.chars().count(), 300);
    assert!(said.starts_with("Serveur injoignable"));
    assert_eq!(failure(&db, second).await, None);

    // Kept: the failure goes.
    let renewed = sealed(31);
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let kept = keep_connector_renewal(
        &mut tx,
        claimed,
        Renewed {
            access_token: &renewed,
            access_expires_at: Some(Utc::now() + chrono::Duration::minutes(2)),
            refresh_token: None,
            scopes: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(kept, Some(first));
    assert_eq!(failure(&db, first).await, None);

    // Renewed since, or given again: a former claim's failure is not said.
    assert!(!fail(&db, claimed, "Trop tard").await);
    let claimed = claim(&db, first).await;
    authorize(&db, first, 32).await;
    assert!(!fail(&db, claimed, "Autorisation remplacée").await);
    assert_eq!(failure(&db, first).await, None);
}

/// Ends the connector's claim, as time would.
async fn end_claim(db: &Database, connector_id: Uuid) {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update connector_authorizations set renewing_until = now() where connector_id = $1",
    )
    .bind(connector_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}
