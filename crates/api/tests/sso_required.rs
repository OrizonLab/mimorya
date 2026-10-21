//! A provider required for its domains (ADR-066): their accounts' password
//! is refused at sign-in (told only to whoever knows it, and counted as no
//! failure), at the acceptance of an invitation and for a reset link; the
//! company's administrators of its sign-in keep theirs. A disabled or no
//! longer required provider leaves passwords be, and another company's
//! provider, required for the same domain, never reaches these accounts.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept, directory_entry, invite_account};
use support::companies::create;
use support::identity_provider::{MockIdentityProvider, Person};
use support::refusals::latest_entry;
use support::resets::ask_reset;
use support::sso::{
    PROVIDERS, SSO_DEPLOYMENT, declaration, declare, discover, location, remove, sign_in,
};
use support::{PASSWORD, TestApi, email};
use uuid::Uuid;

/// Its deployment, logins limited loosely: the account's failures alone
/// count here.
fn deployment() -> Vec<(&'static str, &'static str)> {
    let mut env = SSO_DEPLOYMENT.to_vec();
    env.push(("LOGIN_RATE_LIMIT_PER_MINUTE", "100"));
    env
}

fn required(idp: &MockIdentityProvider) -> serde_json::Value {
    let mut body = declaration(idp);
    body["required"] = json!(true);
    body
}

/// The reasons of the logins refused to this account since `since`.
async fn refusals(api: &TestApi, user_id: Uuid, since: i64) -> Vec<Option<String>> {
    api.idle().await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select reason from audit_log where id > $1 and action = 'auth.login' \
         and outcome = 'denied' and actor_user_id = $2 order by id",
    )
    .bind(since)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_required_provider_refuses_passwords_but_to_its_administrators() {
    let api = TestApi::start_with_mailer_and(&deployment()).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, required(&idp)).await;
    let nadia = email(user::NADIA);
    assert_eq!(discover(&api, &nadia).await["required"], true);

    // The right password: refused, the provider named, never a failure.
    let since = latest_entry(&api).await;
    for attempt in 0..11 {
        let refused = api.try_login(&nadia, PASSWORD).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "attempt {attempt}");
        assert!(
            refused
                .text()
                .contains("impose la connexion par Annuaire Néo")
        );
    }
    let reasons = refusals(&api, user::NADIA, since).await;
    assert_eq!(reasons.len(), 11);
    assert!(
        reasons
            .iter()
            .all(|r| r.as_deref() == Some("Fournisseur d’identité obligatoire"))
    );
    // A wrong one learns nothing of it.
    let wrong = api.try_login(&nadia, "pas-le-bon-mot-de-passe").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert!(!wrong.text().contains("Annuaire"));
    // Its provider signs it in; no reset link is sent for a password it
    // cannot use.
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    let sent = api.mail.sent().len();
    ask_reset(&api, &nadia).await;
    assert_eq!(api.mail.sent().len(), sent);

    // The administrator of the company's sign-in keeps the password, and
    // its reset link.
    api.sign_in(user::PAUL).await;
    ask_reset(&api, &email(user::PAUL)).await;
    assert_eq!(api.mail.sent().len(), sent + 1);

    // No longer required, or disabled: the password serves again.
    let path = format!("{PROVIDERS}/{id}");
    let changed = paul.patch(&path, json!({ "required": false })).await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    api.sign_in(user::NADIA).await;
    paul.patch(&path, json!({ "required": true, "enabled": false }))
        .await;
    api.sign_in(user::NADIA).await;
    let listed = paul.get(PROVIDERS).await.json();
    assert_eq!(listed["providers"][0]["required"], true);
    assert_eq!(listed["providers"][0]["enabled"], false);
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_invitation_under_a_required_provider_is_accepted_through_it_only() {
    let api = TestApi::start_with(&deployment()).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, required(&idp)).await;
    let (invited, invitation) = invite_account(&paul, "obligatoire.sso").await;
    let link = invitation["link"].as_str().unwrap();

    let refused = accept(&api, link, INVITED_PASSWORD).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert!(refused.text().contains("activez votre compte avec lui"));
    assert_eq!(
        directory_entry(&paul, invited.id).await["status"],
        "invited"
    );
    idp.sign_in_as(Person::verified("kc-obligatoire", &invited.email));
    assert_eq!(
        location(&sign_in(&api, &idp, &invited.email).await),
        "/chat"
    );
    assert_eq!(directory_entry(&paul, invited.id).await["status"], "active");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_required_provider_never_reaches_these_accounts() {
    let api = TestApi::start_with(&deployment()).await;
    let idp = MockIdentityProvider::start().await;
    // Another company requires a provider for Néo's very domain.
    let admin = "admin@exigeante.example";
    let other = create(&api, "exigeante", admin).await;
    let accepted = accept(&api, &other.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let theirs = api.sign_in_with(admin, INVITED_PASSWORD).await;
    let id = declare(&theirs, required(&idp)).await;

    // Néo's accounts keep their password, and are offered nothing.
    let nadia = email(user::NADIA);
    assert_eq!(discover(&api, &nadia).await, json!(null));
    api.sign_in(user::NADIA).await;
    remove(&theirs, &id).await;
}
