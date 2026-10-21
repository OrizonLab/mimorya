//! Accepting an invitation through the provider under concurrency
//! (ADR-064): the link and the provider, in either order, and a link resent
//! meanwhile. The account's row is held until both requests wait for it
//! (`support::locks`), then released: the first to get it settles the
//! account, the other finds it active, and one acceptance only is audited.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{INVITED_PASSWORD, NewAccount, accept, directory_entry, invite_account};
use support::identity_provider::{MockIdentityProvider, Person};
use support::locks::{hold_rows, release_after, wait_for_waiters};
use support::refusals::latest_entry;
use support::sso::{
    SSO_DEPLOYMENT, come_back, declaration, declare, invitation_acceptances, location, remove,
    start,
};

/// A provider declared, an account invited, and its return from the
/// provider ready to be sent: everything but the account's row.
struct Race<'a> {
    api: &'a TestApi,
    provider_id: String,
    account: NewAccount,
    link: String,
    /// The provider's return, and the attempt's cookie.
    back: (String, String),
}

async fn prepare<'a>(api: &'a TestApi, idp: &MockIdentityProvider, login: &str) -> Race<'a> {
    let paul = api.sign_in(user::PAUL).await;
    let provider_id = declare(&paul, declaration(idp)).await;
    let (account, invitation) = invite_account(&paul, login).await;
    idp.sign_in_as(Person::verified(&format!("kc-{login}"), &account.email));
    let (url, cookie) = start(api, &account.email).await;
    Race {
        api,
        provider_id,
        link: invitation["link"].as_str().unwrap().to_string(),
        back: (idp.authorize(&url), cookie),
        account,
    }
}

impl Race<'_> {
    async fn come_back(&self) -> String {
        let back = come_back(self.api, &self.back.0, Some(&self.back.1)).await;
        location(&back).to_string()
    }

    async fn end(self) {
        remove(&self.api.sign_in(user::PAUL).await, &self.provider_id).await;
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_provider_first_spends_the_link() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let race = prepare(&api, &idp, "course.sso.devant").await;
    let since = latest_entry(&api).await;

    let hold = hold_rows(&api, "users", &[race.account.id]).await;
    let (signed_in, accepted, ()) = tokio::join!(
        race.come_back(),
        async {
            wait_for_waiters(&api, 1).await;
            accept(&api, &race.link, INVITED_PASSWORD).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(signed_in, "/chat");
    assert_eq!(
        accepted.status,
        StatusCode::NOT_FOUND,
        "{}",
        accepted.text()
    );
    let paul = api.sign_in(user::PAUL).await;
    assert_eq!(
        directory_entry(&paul, race.account.id).await["status"],
        "active"
    );
    let refused = api.try_login(&race.account.email, INVITED_PASSWORD).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        invitation_acceptances(&api, since).await,
        [(
            Some(race.account.id),
            json!({ "method": "sso", "providerId": race.provider_id })
        )]
    );
    race.end().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_link_first_leaves_the_provider_a_member_s_sign_in() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let race = prepare(&api, &idp, "course.lien.devant").await;
    let since = latest_entry(&api).await;

    let hold = hold_rows(&api, "users", &[race.account.id]).await;
    let (accepted, signed_in, ()) = tokio::join!(
        accept(&api, &race.link, INVITED_PASSWORD),
        async {
            wait_for_waiters(&api, 1).await;
            race.come_back().await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    assert_eq!(signed_in, "/chat");
    // Accepted once, by its link: the password chosen there serves.
    let acceptances = invitation_acceptances(&api, since).await;
    assert_eq!(acceptances.len(), 1, "{acceptances:?}");
    assert_eq!(acceptances[0].0, Some(race.account.id));
    assert_eq!(acceptances[0].1.get("method"), None);
    api.sign_in_with(&race.account.email, INVITED_PASSWORD)
        .await;
    race.end().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_resent_meanwhile_never_outlives_the_acceptance() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;

    // Accepted first: the resend then finds an active account.
    let race = prepare(&api, &idp, "course.sso.renvoi").await;
    let resend = format!("/admin/users/{}/invitation", race.account.id);
    let hold = hold_rows(&api, "users", &[race.account.id]).await;
    let (signed_in, resent, ()) = tokio::join!(
        race.come_back(),
        async {
            wait_for_waiters(&api, 1).await;
            paul.post(&resend, json!({})).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(signed_in, "/chat");
    assert_eq!(resent.status, StatusCode::CONFLICT, "{}", resent.text());
    let listed = directory_entry(&paul, race.account.id).await;
    assert_eq!(listed["status"], "active");
    assert_eq!(listed["invitationExpiresAt"], Value::Null);
    race.end().await;

    // Resent first: the acceptance then takes the new link with it.
    let race = prepare(&api, &idp, "course.renvoi.sso").await;
    let resend = format!("/admin/users/{}/invitation", race.account.id);
    let hold = hold_rows(&api, "users", &[race.account.id]).await;
    let (resent, signed_in, ()) = tokio::join!(
        paul.post(&resend, json!({})),
        async {
            wait_for_waiters(&api, 1).await;
            race.come_back().await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(resent.status, StatusCode::OK, "{}", resent.text());
    assert_eq!(signed_in, "/chat");
    let listed = directory_entry(&paul, race.account.id).await;
    assert_eq!(listed["status"], "active");
    assert_eq!(listed["invitationExpiresAt"], Value::Null);
    let new_link = resent.json()["invitation"]["link"]
        .as_str()
        .unwrap()
        .to_string();
    let spent = accept(&api, &new_link, INVITED_PASSWORD).await;
    assert_eq!(spent.status, StatusCode::NOT_FOUND, "{}", spent.text());
    race.end().await;
}
