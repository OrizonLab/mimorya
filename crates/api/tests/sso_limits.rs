//! The steps of a sign-in through an identity provider have their own
//! limits per address (ADR-063). The addresses asked about, by the
//! discovery or the start, each telling whether an address has an account
//! tied to a provider: as many as logins, asking again about one costing
//! nothing. The start and the return together: twice the login limit; past
//! it, a return replayed with its sealed attempt is turned away before the
//! audit or the provider. None spends the password sign-in's limit.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use support::identity_provider::{MockIdentityProvider, Person};
use support::limits::forget_counts;
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
async fn the_start_and_the_return_share_their_limit() {
    let api = TestApi::start_with(&one_a_minute()).await;
    let admin = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = admin.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));

    // Two starts: the return then is a third step, turned away untraced.
    start(&api, &nadia).await;
    let (url, cookie) = start(&api, &nadia).await;
    let since = latest_entry(&api).await;
    assert_refused(&come_back(&api, &idp.authorize(&url), Some(&cookie)).await);
    assert_eq!(idp.exchanges(), 0);
    let audited = sso_logins(&api, mimorya_core::demo::tenant::ID, since).await;
    assert!(audited.is_empty(), "{audited:?}");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_addresses_asked_about_are_as_many_as_logins_by_either_step() {
    // Its own limits: the provider is declared through another server.
    let api = TestApi::start_with(&one_a_minute()).await;
    let admin = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = admin.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let nadia = email(user::NADIA);
    // Shared by both servers (ADR-088): Paul's sign-in there was another
    // client's, from elsewhere; this client's minute starts now.
    forget_counts().await;

    // One address a minute here: asked about again, it costs nothing.
    assert_eq!(discover(&api, &nadia).await["name"], "Annuaire Néo");
    assert_eq!(
        discover(&api, &nadia.to_uppercase()).await["name"],
        "Annuaire Néo"
    );
    let other = "personne@ailleurs.example";
    let refused = try_discover(&api, other).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    // The start tells as much as the discovery: another address is refused
    // there too, not answered as one without a provider.
    let refused = try_start(&api, other).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    // The address asked about starts its sign-in; the password sign-in
    // stays.
    start(&api, &nadia).await;
    api.sign_in(user::PAUL).await;
    remove(&paul, &id).await;
}
