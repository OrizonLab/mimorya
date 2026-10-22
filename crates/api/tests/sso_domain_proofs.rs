//! Proving the company's e-mail domains (ADR-069): each domain its providers
//! cover is given a DNS record of the company's own, read on request. Absent,
//! another company's, unreadable, or for a domain no provider covers, the
//! proof is refused. A generic provider then creates accounts in a
//! proved domain only: another company declaring Néo's domain on a provider
//! of its own takes none of its addresses.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_core::identity_providers::{proof_record_name, proof_record_value};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies::create;
use support::identity_provider::{MockIdentityProvider, Person};
use support::refusals::{latest_entry, latest_entry_of, refusals_since};
use support::sso::{
    DOMAIN, DOMAIN_PROOFS, PROVIDERS, SSO_DEPLOYMENT, assert_refused, declaration, declare,
    location, prove_domain, remove, sign_in_by_link, sso_logins,
};
use support::{Client, TestApi, email};
use uuid::Uuid;

const NEWCOMER: &str = "noe.arrivant@neo-maintenance.example";
/// Domains no other test proves.
const PROVED: &str = "neo-preuve.example";
const ARRIVALS: &str = "neo-arrivants.example";
const NOT_PROVED: &str = "Domaine non prouvé par l’entreprise : aucun compte n’y est créé";

fn creating(idp: &MockIdentityProvider) -> Value {
    let mut body = declaration(idp);
    body["createAccounts"] = json!(true);
    body
}

/// The company's single domain, as « Connexion » lists it.
async fn proof_of(admin: &Client<'_>) -> Value {
    let listed = admin.get(PROVIDERS).await.json();
    assert_eq!(
        listed["domains"].as_array().map(Vec::len),
        Some(1),
        "{listed}"
    );
    listed["domains"][0].clone()
}

/// The proofs audited in the demo company since `since`.
async fn proofs_audited(api: &TestApi, since: i64) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select metadata from audit_log \
         where id > $1 and action = 'identity_provider.domain_verified' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_domain_is_proved_by_the_company_s_own_record() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    // A domain of its own: proofs outlive the test that made them.
    let mut body = declaration(&idp);
    body["domains"] = json!([PROVED]);
    let id = declare(&paul, body).await;
    let name = proof_record_name(PROVED);
    let value = proof_record_value(tenant::ID, PROVED);
    // Given with the record that proves it, not proved yet.
    assert_eq!(
        proof_of(&paul).await,
        json!({ "domain": PROVED, "recordName": name, "recordValue": value, "verifiedAt": null })
    );
    let attempt = |domain: &str| paul.post(DOMAIN_PROOFS, json!({ "domain": domain }));

    // Not published, another company's, unreadable: refused.
    let since = latest_entry(&api).await;
    let absent = attempt(PROVED).await;
    assert_eq!(absent.status, StatusCode::BAD_REQUEST);
    let expected = format!("aucun enregistrement TXT {name} ne porte la valeur attendue");
    assert!(
        absent.error_message().contains(&expected),
        "{}",
        absent.text()
    );
    api.dns
        .publish(&name, &proof_record_value(Uuid::from_u128(7), PROVED));
    assert_eq!(attempt(PROVED).await.status, StatusCode::BAD_REQUEST);
    api.dns.fail(true);
    let unreadable = attempt(PROVED).await;
    api.dns.fail(false);
    assert_eq!(unreadable.status, StatusCode::BAD_REQUEST);
    assert!(unreadable.error_message().contains("n’a pas pu être lu"));
    // A domain no provider of the company covers, even with its record.
    let elsewhere = "ailleurs.example";
    api.dns.publish(
        &proof_record_name(elsewhere),
        &proof_record_value(tenant::ID, elsewhere),
    );
    let uncovered = attempt(elsewhere).await;
    assert_eq!(uncovered.status, StatusCode::BAD_REQUEST);
    assert!(uncovered.error_message().contains("aucun fournisseur"));
    assert!(proofs_audited(&api, since).await.is_empty());

    // Published, but asked by someone without the right: refused, traced,
    // nothing proved.
    api.dns.publish(&name, &value);
    let marc = api.sign_in(user::MARC).await;
    let unauthorised = marc.post(DOMAIN_PROOFS, json!({ "domain": PROVED })).await;
    assert_eq!(unauthorised.status, StatusCode::FORBIDDEN);
    assert_eq!(refusals_since(&api, since).await.len(), 1);
    assert_eq!(proof_of(&paul).await["verifiedAt"], Value::Null);

    // Asked by its administrator: proved, dated, audited; asked again, the
    // date moves.
    let proved = attempt(PROVED).await;
    assert_eq!(proved.status, StatusCode::OK, "{}", proved.text());
    let first = proof_of(&paul).await["verifiedAt"].clone();
    assert!(first.is_string(), "{first}");
    assert_eq!(attempt(PROVED).await.status, StatusCode::OK);
    assert_eq!(
        proofs_audited(&api, since).await,
        [json!({ "domain": PROVED }), json!({ "domain": PROVED })]
    );
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_generic_provider_creates_accounts_in_a_proved_domain_only() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    // Néo's domain, which holds Nadia's account, and a new one, not proved.
    let mut body = creating(&idp);
    body["domains"] = json!([DOMAIN, ARRIVALS]);
    let id = declare(&paul, body).await;
    let newcomer = format!("noe.arrivant@{ARRIVALS}");
    idp.sign_in_as(Person::verified("kc-noe", &newcomer));
    let since = latest_entry(&api).await;
    assert_refused(&sign_in_by_link(&api, &idp, &id).await);
    let logins = sso_logins(&api, tenant::ID, since).await;
    assert_eq!(logins[0].2.as_deref(), Some(NOT_PROVED));

    // An account the company holds still signs in: creation alone waits for
    // the proof.
    idp.sign_in_as(Person::verified("kc-nadia", &email(user::NADIA)));
    assert_eq!(location(&sign_in_by_link(&api, &idp, &id).await), "/chat");

    prove_domain(&api, &paul, tenant::ID, ARRIVALS).await;
    idp.sign_in_as(Person::verified("kc-noe", &newcomer));
    assert_eq!(location(&sign_in_by_link(&api, &idp, &id).await), "/chat");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_takes_no_address_of_a_domain_it_does_not_hold() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    // Néo's DNS carries Néo's record only.
    api.dns.publish(
        &proof_record_name(DOMAIN),
        &proof_record_value(tenant::ID, DOMAIN),
    );
    // A neighbour declares Néo's domain on a provider of its own, told to
    // create accounts.
    let admin = "admin@voisine-dns.example";
    let other = create(&api, "voisine-dns", admin).await;
    let accepted = accept(&api, &other.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let theirs = api.sign_in_with(admin, INVITED_PASSWORD).await;
    let their_id = declare(&theirs, creating(&idp)).await;
    let claimed = theirs
        .post(DOMAIN_PROOFS, json!({ "domain": DOMAIN }))
        .await;
    assert_eq!(
        claimed.status,
        StatusCode::BAD_REQUEST,
        "{}",
        claimed.text()
    );

    // Its provider vouches for a Néo address: nothing is created there.
    idp.sign_in_as(Person::verified("their-noe", NEWCOMER));
    let since = latest_entry_of(&api, other.tenant_id).await;
    assert_refused(&sign_in_by_link(&api, &idp, &their_id).await);
    let logins = sso_logins(&api, other.tenant_id, since).await;
    assert_eq!(logins[0].2.as_deref(), Some(NOT_PROVED));

    // The address is still free for Néo, which proves its domain.
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, creating(&idp)).await;
    prove_domain(&api, &paul, tenant::ID, DOMAIN).await;
    idp.sign_in_as(Person::verified("kc-noe", NEWCOMER));
    assert_eq!(location(&sign_in_by_link(&api, &idp, &id).await), "/chat");
    remove(&paul, &id).await;
    remove(&theirs, &their_id).await;
}
