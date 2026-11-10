//! Accounts linked at Google and let go of by the platform in the tests
//! (ADR-194): a person's Google account linked at the local provider, and
//! what the audit keeps of the revocations, never a token.

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::Value;
use uuid::Uuid;

use super::account_links::{LINKED, google_person, link};
use super::identity_provider::MockIdentityProvider;
use super::sso::location;
use super::{Client, TestApi, email};

/// The revocations audited after `since`: actor, outcome, connector,
/// reason, counts.
pub async fn revocations(
    api: &TestApi,
    since: i64,
) -> Vec<(Option<Uuid>, String, Option<String>, Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select actor_user_id, outcome, resource_id, reason, metadata from audit_log \
         where id > $1 and action = 'account.links_revoked' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

/// The last audit entry's place.
pub async fn last_entry(api: &TestApi) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let last = sqlx::query_scalar("select coalesce(max(id), 0) from audit_log")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    last
}

/// No refresh token, nor anything Google wrote around its error code,
/// anywhere in the company's audit.
pub async fn no_token_audited(api: &TestApi) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let found: i64 = sqlx::query_scalar(
        "select count(*) from audit_log where coalesce(reason, '') like '%jeton-de-renouvellement%' \
         or metadata::text like '%jeton-de-renouvellement%'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(found, 0);
}

pub fn domain_of(address: &str) -> String {
    address.rsplit_once('@').unwrap().1.to_string()
}

/// Links `who`'s Google account, its subject `subject`.
pub async fn link_google<'a>(
    api: &'a TestApi,
    idp: &MockIdentityProvider,
    connector: Uuid,
    who: Uuid,
    subject: &str,
) -> Client<'a> {
    let person = api.sign_in(who).await;
    link_google_as(&person, idp, connector, &email(who), subject).await;
    person
}

/// Links the Google account of `person`, whose address is `address`.
pub async fn link_google_as(
    person: &Client<'_>,
    idp: &MockIdentityProvider,
    connector: Uuid,
    address: &str,
    subject: &str,
) {
    idp.sign_in_as(google_person(subject, address, Some(&domain_of(address))));
    assert_eq!(location(&link(person, idp, connector).await), LINKED);
}

pub async fn remove_connectors(api: &TestApi, connectors: &[Uuid]) {
    let paul = api.sign_in(user::PAUL).await;
    for connector in connectors {
        let removed = paul.delete(&format!("/admin/connectors/{connector}")).await;
        assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    }
}
