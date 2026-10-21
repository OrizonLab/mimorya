//! A provider's declaration and keys are kept an hour (ADR-065), per
//! declared provider: sign-ins read them from memory, not from the
//! provider. A signature the kept keys fail (keys rolled over, with or
//! without identifiers) reads them again once, and a token they still fail
//! is refused after that one read; no other refusal reads them again. A
//! failure is not kept, nor a declaration too heavy, nor what an
//! administrator's check reads; companies sharing an issuer keep it apart.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies::create;
use support::identity_provider::{MockIdentityProvider, Person, Tamper};
use support::refusals::latest_entry;
use support::sso::{
    PROVIDERS, SSO_DEPLOYMENT, assert_refused, come_back, declaration, declare, location, remove,
    sign_in, sso_logins, start, try_start,
};
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn sign_ins_read_the_kept_declaration_and_keys() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    // Checked by the administration: read, not kept.
    let id = declare(&paul, declaration(&idp)).await;
    assert_eq!(idp.reads(), (1, 1));

    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    for _ in 0..2 {
        assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    }
    assert_eq!(idp.exchanges(), 2);
    assert_eq!(idp.reads(), (2, 2));
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn keys_rolled_over_are_read_again_once() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");

    // Signed by a key the kept ones lack: read again, then verified.
    idp.roll_keys_over();
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    assert_eq!(idp.reads(), (3, 3));
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    assert_eq!(idp.reads(), (3, 3));

    // Refused for another reason than its signature: nothing read again.
    idp.tamper(Tamper {
        audience: Some("une-autre-application".into()),
        ..Tamper::default()
    });
    assert_refused(&sign_in(&api, &idp, &nadia).await);
    assert_eq!(idp.reads(), (3, 3));

    // Signed by a key it does not publish: read once more, then refused.
    idp.tamper(Tamper {
        unpublished_key: true,
        ..Tamper::default()
    });
    let since = latest_entry(&api).await;
    assert_refused(&sign_in(&api, &idp, &nadia).await);
    assert_eq!(idp.reads(), (4, 4));
    let refused = sso_logins(&api, tenant::ID, since).await;
    assert_eq!(
        refused,
        [(
            "denied".to_string(),
            None,
            Some("jeton d’identité : clé de signature inconnue".to_string())
        )]
    );
    // Returning to a server that kept nothing (started on another), its
    // declaration is read just now: its keys are not read twice.
    let fresh = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let (url, cookie) = start(&api, &nadia).await;
    assert_refused(&come_back(&fresh, &idp.authorize(&url), Some(&cookie)).await);
    assert_eq!(idp.reads(), (5, 5));
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn keys_rolled_over_without_identifiers_are_read_again_once() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    idp.without_key_ids();
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");

    // The kept key is tried and fails: read again, then verified.
    idp.roll_keys_over();
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    assert_eq!(idp.reads(), (3, 3));
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_provider_that_failed_is_asked_again() {
    let admin = TestApi::start_with(&SSO_DEPLOYMENT).await;
    // Another server: nothing kept yet.
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = admin.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);

    idp.set_unavailable(true);
    let refused = try_start(&api, &nadia).await;
    assert_eq!(
        refused.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        refused.text()
    );
    idp.set_unavailable(false);
    start(&api, &nadia).await;
    // Read there, then kept: the provider unavailable again, it still starts.
    idp.set_unavailable(true);
    start(&api, &nadia).await;
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_declaration_too_heavy_is_read_at_each_sign_in() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    idp.publish_extra_keys(200);
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    // Read at the start and at the return, as before it was kept.
    assert_eq!(idp.reads(), (3, 3));
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn companies_sharing_an_issuer_keep_its_declaration_apart() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let neo = declare(&paul, declaration(&idp)).await;
    // Another company declares the same issuer for its own domain.
    let admin = "admin@partage.example";
    let other = create(&api, "partage", admin).await;
    let accepted = accept(&api, &other.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let theirs = api.sign_in_with(admin, INVITED_PASSWORD).await;
    let mut body = declaration(&idp);
    body["domains"] = json!(["partage.example"]);
    let id = declare(&theirs, body).await;
    assert_eq!(idp.reads(), (2, 2));

    // Kept for Néo's provider, it is read again for the other's.
    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    assert_eq!(idp.reads(), (3, 3));
    idp.sign_in_as(Person::verified("kc-partage", admin));
    assert_eq!(location(&sign_in(&api, &idp, admin).await), "/chat");
    assert_eq!(idp.reads(), (4, 4));
    assert_eq!(location(&sign_in(&api, &idp, admin).await), "/chat");
    assert_eq!(idp.reads(), (4, 4));
    remove(&theirs, &id).await;
    remove(&paul, &neo).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_s_declaration_always_reads_the_provider() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    start(&api, &email(user::NADIA)).await;
    remove(&paul, &id).await;

    // Its declaration kept by a start, yet asked again: unavailable, refused.
    idp.set_unavailable(true);
    let declared = paul.post(PROVIDERS, declaration(&idp)).await;
    assert_eq!(
        declared.status,
        StatusCode::BAD_REQUEST,
        "{}",
        declared.text()
    );
    assert!(declared.text().contains("Fournisseur injoignable"));
    assert_eq!(idp.reads().0, 3);
}
