//! An invited account accepts its invitation through its company's identity
//! provider (ADR-064): offered the provider like an active one, it becomes
//! active without a password at its first sign-in there, its invitation link
//! spent, and the acceptance audited; only through an address the provider
//! vouches for, and never once disabled.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept, directory_entry, invite_account};
use support::identity_provider::{MockIdentityProvider, Person};
use support::refusals::latest_entry;
use support::sso::{
    SSO_DEPLOYMENT, assert_refused, declaration, declare, discover, location, remove, session,
    sign_in, sso_logins, try_start,
};
use support::{PASSWORD, TestApi};

/// The acceptances audited since `since`: account, metadata.
async fn acceptances(api: &TestApi, since: i64) -> Vec<(Option<uuid::Uuid>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select actor_user_id, metadata from audit_log \
         where id > $1 and action = 'user.invitation_accepted' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
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

    // Active, without a password; its invitation deleted, its link spent.
    assert_eq!(directory_entry(&paul, invited.id).await["status"], "active");
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let pending: i64 =
        sqlx::query_scalar("select count(*) from user_invitations where user_id = $1")
            .bind(invited.id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(pending, 0);
    let spent = accept(&api, &link, INVITED_PASSWORD).await;
    assert_eq!(spent.status, StatusCode::NOT_FOUND, "{}", spent.text());
    let by_password = api.try_login(&invited.email, PASSWORD).await;
    assert_eq!(by_password.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        acceptances(&api, since).await,
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
    assert!(acceptances(&api, since).await.is_empty());
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
    assert!(acceptances(&api, since).await.is_empty());

    // Disabled, it is offered nothing, and no sign-in starts.
    let account = format!("/admin/users/{}", invited.id);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(discover(&api, &invited.email).await, json!(null));
    let refused = try_start(&api, &invited.email).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    assert!(invitation["link"].is_string());
    remove(&paul, &id).await;
}
