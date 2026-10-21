//! The steps of a sign-in through an identity provider (discovery, start,
//! return) have their own limit per address, three times the sign-in limit
//! (ADR-063): past it, a return replayed with its sealed attempt is turned
//! away before the audit or the provider; and the discovery the sign-in page
//! makes for every address spends nothing of the password sign-in limit.

mod support;

use mimorya_core::demo::user;
use serde_json::json;
use support::identity_provider::{MockIdentityProvider, Person};
use support::refusals::latest_entry;
use support::sso::{
    SSO_DEPLOYMENT, assert_refused, come_back, declaration, declare, discover, remove, sso_logins,
    start,
};
use support::{TestApi, email};

/// One sign-in a minute per address: three steps of single sign-on.
fn one_a_minute() -> Vec<(&'static str, &'static str)> {
    let mut env = SSO_DEPLOYMENT.to_vec();
    env.push(("LOGIN_RATE_LIMIT_PER_MINUTE", "1"));
    env
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_replayed_return_is_turned_away_past_the_limit() {
    let api = TestApi::start_with(&one_a_minute()).await;
    let idp = MockIdentityProvider::start().await;
    // Paul's password sign-in spends the minute's sign-in, not the steps.
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));

    let (url, cookie) = start(&api, &nadia).await;
    let forged = "/auth/sso/callback?code=invente&state=autre";
    let since = latest_entry(&api).await;
    assert_refused(&come_back(&api, forged, Some(&cookie)).await);
    assert_refused(&come_back(&api, forged, Some(&cookie)).await);
    // The fourth step: neither audited nor sent on, even the right return.
    let requests = idp.requests();
    assert_refused(&come_back(&api, &idp.authorize(&url), Some(&cookie)).await);
    assert_eq!(idp.requests(), requests);
    assert_eq!(idp.exchanges(), 0);
    let audited = sso_logins(&api, mimorya_core::demo::tenant::ID, since).await;
    assert_eq!(audited.len(), 2, "{audited:?}");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn discovering_spends_nothing_of_the_password_sign_in_limit() {
    let api = TestApi::start_with(&one_a_minute()).await;
    for _ in 0..3 {
        assert_eq!(discover(&api, &email(user::NADIA)).await, json!(null));
    }
    // The minute's one password sign-in is still there.
    api.sign_in(user::PAUL).await;
}
