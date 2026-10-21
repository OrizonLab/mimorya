//! A provider's declaration and keys are kept an hour (ADR-065): sign-ins
//! read them from memory, not from the provider; keys it rolled over are
//! read again once, and a token signed by none of them is refused after
//! that one read; a failure is not kept; an administrator's declaration
//! always reads the provider anew.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
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
    // Declared: read once, as the administration checks it.
    let id = declare(&paul, declaration(&idp)).await;
    assert_eq!(idp.reads(), (1, 1));

    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    for _ in 0..2 {
        assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    }
    assert_eq!(idp.exchanges(), 2);
    assert_eq!(idp.reads(), (1, 1));
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

    // Signed by a key the kept ones lack: read again, then verified.
    idp.roll_keys_over();
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    assert_eq!(idp.reads(), (2, 2));
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    assert_eq!(idp.reads(), (2, 2));

    // Signed by a key it does not publish: read once more, then refused.
    idp.tamper(Tamper {
        unpublished_key: true,
        ..Tamper::default()
    });
    let since = latest_entry(&api).await;
    assert_refused(&sign_in(&api, &idp, &nadia).await);
    assert_eq!(idp.reads(), (3, 3));
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
    assert_eq!(idp.reads(), (4, 4));
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
async fn an_administrator_s_declaration_always_reads_the_provider() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    remove(&paul, &id).await;

    // Kept by the first declaration, yet asked again: unavailable, refused.
    idp.set_unavailable(true);
    let declared = paul.post(PROVIDERS, declaration(&idp)).await;
    assert_eq!(
        declared.status,
        StatusCode::BAD_REQUEST,
        "{}",
        declared.text()
    );
    assert!(declared.text().contains("Fournisseur injoignable"));
    assert_eq!(idp.reads().0, 2);
}
