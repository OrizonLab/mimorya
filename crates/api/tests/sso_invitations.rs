//! An invited account accepts its invitation through its company's identity
//! provider (ADR-064): offered the provider like an active one, it becomes
//! active without a password at its first sign-in there, its invitation link
//! spent, and the acceptance audited, even past the link's expiry; only
//! through an address the provider vouches for, and never once disabled.
//! Disabled then enabled again, it stays active, with nothing to accept.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept, directory_entry, invite_account};
use support::identity_provider::{MockIdentityProvider, Person};
use support::refusals::latest_entry;
use support::sso::{
    SSO_DEPLOYMENT, assert_refused, declaration, declare, discover, invitation_acceptances,
    location, remove, session, sign_in, sso_logins, try_start,
};
use support::{PASSWORD, TestApi};
use uuid::Uuid;

/// What the account's row holds: a password, and its invitations pending.
async fn stored(api: &TestApi, user_id: Uuid) -> (bool, i64) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let state = sqlx::query_as(
        "select u.password_hash is not null, \
         (select count(*) from user_invitations i where i.user_id = u.id) \
         from users u where u.id = $1",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    state
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_invited_account_accepts_its_invitation_through_the_provider() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let (invited, invitation) = invite_account(&paul, "accepte.sso").await;
    let link = invitation["link"].as_str().unwrap().to_string();

    // Offered like an active account.
    let offered = discover(&api, &invited.email).await;
    assert_eq!(offered["name"], "Annuaire Néo");
    let since = latest_entry(&api).await;
    idp.sign_in_as(Person::verified("kc-accepte", &invited.email));
    let back = sign_in(&api, &idp, &invited.email).await;
    assert_eq!(location(&back), "/chat");
    let token = session(&back).expect("session");
    let me = api.as_user(&token).get("/auth/me").await.json();
    assert_eq!(me["user"]["id"], json!(invited.id));
    assert_eq!(me["user"]["mustChangePassword"], false);
    assert_eq!(me["user"]["hasPassword"], false);

    // Active, without a password; its invitation deleted, its link spent.
    assert_eq!(directory_entry(&paul, invited.id).await["status"], "active");
    assert_eq!(stored(&api, invited.id).await, (false, 0));
    let spent = accept(&api, &link, INVITED_PASSWORD).await;
    assert_eq!(spent.status, StatusCode::NOT_FOUND, "{}", spent.text());
    let by_password = api.try_login(&invited.email, PASSWORD).await;
    assert_eq!(by_password.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        invitation_acceptances(&api, since).await,
        [(
            Some(invited.id),
            json!({ "method": "sso", "providerId": id })
        )]
    );
    assert_eq!(
        sso_logins(&api, tenant::ID, since).await,
        [("success".to_string(), Some(invited.id), None)]
    );

    // Later sign-ins are a member's: nothing more to accept.
    let since = latest_entry(&api).await;
    assert_eq!(
        location(&sign_in(&api, &idp, &invited.email).await),
        "/chat"
    );
    assert!(invitation_acceptances(&api, since).await.is_empty());
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_invitation_past_its_link_s_expiry_is_accepted_through_the_provider() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let (invited, invitation) = invite_account(&paul, "expire.sso").await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update user_invitations set expires_at = now() - interval '1 day' where user_id = $1",
    )
    .bind(invited.id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let link = invitation["link"].as_str().unwrap();
    let expired = accept(&api, link, INVITED_PASSWORD).await;
    assert_eq!(expired.status, StatusCode::NOT_FOUND, "{}", expired.text());

    // The provider vouches for the address the invitation went to.
    idp.sign_in_as(Person::verified("kc-expire", &invited.email));
    let back = sign_in(&api, &idp, &invited.email).await;
    assert_eq!(location(&back), "/chat");
    assert_eq!(directory_entry(&paul, invited.id).await["status"], "active");
    assert_eq!(stored(&api, invited.id).await, (false, 0));
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_invitation_is_accepted_only_by_a_vouched_address_and_never_once_disabled() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let (invited, invitation) = invite_account(&paul, "refuse.sso").await;

    // An address the provider does not vouch for: still invited.
    let since = latest_entry(&api).await;
    idp.sign_in_as(Person {
        email_verified: Some(false),
        ..Person::verified("kc-refuse", &invited.email)
    });
    assert_refused(&sign_in(&api, &idp, &invited.email).await);
    assert_eq!(
        directory_entry(&paul, invited.id).await["status"],
        "invited"
    );
    assert!(invitation_acceptances(&api, since).await.is_empty());

    // Disabled, it is offered nothing, no sign-in starts, nor does its link
    // serve.
    let account = format!("/admin/users/{}", invited.id);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(discover(&api, &invited.email).await, json!(null));
    let refused = try_start(&api, &invited.email).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    let link = invitation["link"].as_str().unwrap();
    let spent = accept(&api, link, INVITED_PASSWORD).await;
    assert_eq!(spent.status, StatusCode::NOT_FOUND, "{}", spent.text());
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_accepted_through_the_provider_stays_active_once_enabled_again() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    let (invited, _) = invite_account(&paul, "reactive.sso").await;
    idp.sign_in_as(Person::verified("kc-reactive", &invited.email));
    let back = sign_in(&api, &idp, &invited.email).await;
    let own = api.as_user(&session(&back).expect("session"));

    // No password to change: refused each time, never counted as a guess.
    let change = json!({ "currentPassword": "devine", "newPassword": "nouvelle-phrase-de-passe" });
    for attempt in 0..11 {
        let refused = own.post("/account/password", change.clone()).await;
        assert_eq!(refused.status, StatusCode::CONFLICT, "attempt {attempt}");
    }

    // Disabled then enabled again: active, as it was.
    let account = format!("/admin/users/{}", invited.id);
    for status in ["disabled", "active"] {
        let changed = paul.patch(&account, json!({ "status": status })).await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    }
    assert_eq!(directory_entry(&paul, invited.id).await["status"], "active");
    let since = latest_entry(&api).await;
    assert_eq!(
        location(&sign_in(&api, &idp, &invited.email).await),
        "/chat"
    );
    assert!(invitation_acceptances(&api, since).await.is_empty());

    // The administration may give it a temporary password.
    let reset = paul.post(&format!("{account}/password"), json!({})).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    api.sign_in_with(&invited.email, &temporary).await;
    remove(&paul, &id).await;
}
