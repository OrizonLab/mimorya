//! The steps of a sign-in through an identity provider have their own
//! limits per address (ADR-063): the discovery, which tells whether an
//! address has an account tied to a provider, as many as logins; the start
//! and the return together, twice that. Past it, a return replayed with its
//! sealed attempt is turned away before the audit or the provider; and none
//! of them spends the others' limit, nor the password sign-in's.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use support::identity_provider::{MockIdentityProvider, Person};
use support::refusals::latest_entry;
use support::sso::{
    SSO_DEPLOYMENT, assert_refused, come_back, declaration, declare, discover, remove, sso_logins,
    start, try_discover, try_start,
};
use support::{TestApi, email};

/// One sign-in a minute per address: one discovery, two other steps.
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
    // The third step: neither audited nor sent on, even the right return.
    let requests = idp.requests();
    assert_refused(&come_back(&api, &idp.authorize(&url), Some(&cookie)).await);
    assert_eq!(idp.requests(), requests);
    assert_eq!(idp.exchanges(), 0);
    let audited = sso_logins(&api, mimorya_core::demo::tenant::ID, since).await;
    assert_eq!(audited.len(), 1, "{audited:?}");
    let refused = try_start(&api, &nadia).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn discovering_is_limited_as_logins_are_and_apart_from_the_other_steps() {
    // Its own limits: the provider is declared through another server.
    let api = TestApi::start_with(&one_a_minute()).await;
    let admin = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = admin.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);

    // As many discoveries as logins: one a minute here, whatever the address.
    assert_eq!(discover(&api, &nadia).await["name"], "Annuaire Néo");
    let refused = try_discover(&api, "personne@ailleurs.example").await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    // Spent, they leave the start and the minute's password sign-in.
    start(&api, &nadia).await;
    api.sign_in(user::PAUL).await;
    remove(&paul, &id).await;
}
