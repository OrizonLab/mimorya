//! A Google account's grant revoked at Google when the platform lets go of
//! its link (ADR-194): removed by the person, their account disabled, the
//! connector deleted; sent to the revocation endpoint Google's discovery
//! names, audited counted, never a token. A provider that refuses or names
//! no endpoint keeps the grant, said; the removal stands. Microsoft revokes
//! no single grant: nothing is sent there.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::account_links::{
    LINKED, LINKS, google_person, insert_google_workspace, insert_microsoft365, kept_refresh_token,
    link, microsoft_person,
};
use support::identity_provider::MockIdentityProvider;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{Client, TestApi, email};
use uuid::Uuid;

/// The revocations audited after `since`: actor, outcome, connector,
/// reason, counts.
async fn revocations(
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
async fn last_entry(api: &TestApi) -> i64 {
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
async fn no_token_audited(api: &TestApi) {
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

fn domain_of(address: &str) -> String {
    address.rsplit_once('@').unwrap().1.to_string()
}

/// Links `who`'s Google account, its subject `subject`.
async fn link_google<'a>(
    api: &'a TestApi,
    idp: &MockIdentityProvider,
    connector: Uuid,
    who: Uuid,
    subject: &str,
) -> Client<'a> {
    let person = api.sign_in(who).await;
    let address = email(who);
    idp.sign_in_as(google_person(subject, &address, Some(&domain_of(&address))));
    assert_eq!(location(&link(&person, idp, connector).await), LINKED);
    person
}

async fn remove_connectors(api: &TestApi, connectors: &[Uuid]) {
    let paul = api.sign_in(user::PAUL).await;
    for connector in connectors {
        let removed = paul.delete(&format!("/admin/connectors/{connector}")).await;
        assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_google_link_removed_is_revoked_at_google_and_a_microsoft_one_is_not() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let microsoft = insert_microsoft365(&api, &idp, true).await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    assert_eq!(location(&link(&claire, &idp, microsoft).await), LINKED);
    let since = last_entry(&api).await;

    // Her Google link removed: its grant revoked at Google, by the time
    // she is answered.
    let removed = claire.delete(&format!("{LINKS}/{google}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(idp.revoked(), ["jeton-de-renouvellement-g-claire"]);
    assert_eq!(kept_refresh_token(&api, google, user::CLAIRE).await, None);
    assert_eq!(
        revocations(&api, since).await,
        [(
            Some(user::CLAIRE),
            "success".to_string(),
            Some(google.to_string()),
            None,
            json!({ "revoked": 1, "failed": 0 }),
        )]
    );

    // Her Microsoft link removed: Microsoft revokes no single grant,
    // nothing is sent.
    let removed = claire.delete(&format!("{LINKS}/{microsoft}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(idp.revoked().len(), 1);
    assert_eq!(revocations(&api, since).await.len(), 1);
    no_token_audited(&api).await;
    remove_connectors(&api, &[microsoft, google]).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_grant_google_does_not_revoke_is_said_and_the_link_still_goes() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    let marc = link_google(&api, &idp, google, user::MARC, "g-marc").await;
    let since = last_entry(&api).await;

    // Google refuses: the link goes all the same, the refusal said by its
    // code alone.
    idp.refuse_revocations();
    let removed = claire.delete(&format!("{LINKS}/{google}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(kept_refresh_token(&api, google, user::CLAIRE).await, None);
    assert!(idp.revoked().is_empty());

    // No endpoint named: said too.
    idp.name_no_revocation_endpoint();
    let removed = marc.delete(&format!("{LINKS}/{google}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(kept_refresh_token(&api, google, user::MARC).await, None);
    let failed = json!({ "revoked": 0, "failed": 1 });
    assert_eq!(
        revocations(&api, since).await,
        [
            (
                Some(user::CLAIRE),
                "failure".to_string(),
                Some(google.to_string()),
                Some("Google a refusé la révocation (temporarily_unavailable)".to_string()),
                failed.clone(),
            ),
            (
                Some(user::MARC),
                "failure".to_string(),
                Some(google.to_string()),
                Some(
                    "Google ne nomme aucun point de révocation : retirez l’accord dans le \
                     compte Google"
                        .to_string()
                ),
                failed,
            ),
        ]
    );
    no_token_audited(&api).await;
    remove_connectors(&api, &[google]).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_account_and_a_deleted_connector_revoke_their_google_grants() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    link_google(&api, &idp, google, user::NADIA, "g-nadia").await;
    link_google(&api, &idp, google, user::MARC, "g-marc").await;
    let paul = api.sign_in(user::PAUL).await;
    let since = last_entry(&api).await;

    // Nadia leaves: her grant revoked, audited as let go by Paul.
    let account = format!("/admin/users/{}", user::NADIA);
    let left = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(left.status, StatusCode::OK, "{}", left.text());
    assert_eq!(idp.revoked(), ["jeton-de-renouvellement-g-nadia"]);
    let back = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.text());

    // The connector deleted: everyone else's revoked.
    remove_connectors(&api, &[google]).await;
    let mut revoked = idp.revoked();
    revoked.sort();
    assert_eq!(
        revoked,
        [
            "jeton-de-renouvellement-g-claire",
            "jeton-de-renouvellement-g-marc",
            "jeton-de-renouvellement-g-nadia",
        ]
    );
    let entries = revocations(&api, since).await;
    let by_paul = |count: u64| {
        (
            Some(user::PAUL),
            "success".to_string(),
            Some(google.to_string()),
            None,
            json!({ "revoked": count, "failed": 0 }),
        )
    };
    assert_eq!(entries, [by_paul(1), by_paul(2)]);
    no_token_audited(&api).await;
}
