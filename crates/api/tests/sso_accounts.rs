//! Accounts created at the first sign-in (ADR-067): through the company's
//! sign-in link, a provider that creates accounts makes one for a person of
//! its domains whom the company has not invited: active, without a
//! password, a role or a unit, named as the provider names them, audited.
//! Never unless the provider says so, never outside its domains nor for an
//! address it does not vouch for, never over another company's account; a
//! link names only an enabled provider of an active company.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::companies::create;
use support::identity_provider::{MockIdentityProvider, Person};
use support::refusals::latest_entry;
use support::sso::{
    PROVIDERS, SSO_DEPLOYMENT, assert_refused, declaration, declare, location, remove, session,
    sign_in_by_link, sso_logins, try_link, try_start_link,
};
use support::{Client, TestApi, WEB_ORIGIN, owner_url};
use uuid::Uuid;

const NEW: &str = "zoe.nouvelle@neo-maintenance.example";

fn creating(idp: &MockIdentityProvider) -> Value {
    let mut body = declaration(idp);
    body["createAccounts"] = json!(true);
    body
}

/// Someone the provider vouches for, named by these claims.
fn person(subject: &str, email: &str, claims: Value) -> Person {
    let mut person = Person::verified(subject, email);
    if let Value::Object(claims) = claims {
        person.extra = claims;
    }
    person
}

/// The company's account with this address, as its directory lists it.
async fn listed(admin: &Client<'_>, email: &str) -> Option<Value> {
    admin.get("/admin/users").await.json()["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["email"] == email)
        .cloned()
}

/// The accounts created at a sign-in since `since`: account, metadata.
async fn creations(api: &TestApi, since: i64) -> Vec<(Option<Uuid>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select actor_user_id, metadata from audit_log \
         where id > $1 and action = 'user.created' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_link_creates_the_account_of_a_person_of_its_domains() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, creating(&idp)).await;

    // The page names the company and its provider.
    // Handed out by the administration.
    let listed_provider = paul.get(PROVIDERS).await.json();
    assert_eq!(
        listed_provider["providers"][0]["signInLink"],
        format!("{WEB_ORIGIN}/login/entreprise#{id}")
    );
    let link = try_link(&api, &id).await;
    assert_eq!(link.status, StatusCode::OK, "{}", link.text());
    assert_eq!(
        link.json(),
        json!({
            "company": tenant::NAME,
            "provider": { "kind": "oidc", "name": "Annuaire Néo", "required": false },
            "createsAccounts": true,
        })
    );

    let since = latest_entry(&api).await;
    idp.sign_in_as(person("kc-zoe", NEW, json!({ "name": "Zoé Nouvelle" })));
    let back = sign_in_by_link(&api, &idp, &id).await;
    assert_eq!(location(&back), "/chat");
    let zoe = api.as_user(&session(&back).expect("session"));
    let me = zoe.get("/auth/me").await.json();
    assert_eq!(me["user"]["email"], NEW);
    assert_eq!(me["user"]["displayName"], "Zoé Nouvelle");
    assert_eq!(me["user"]["hasPassword"], false);
    // Active, without a role nor a unit, of the default clearance: no right
    // until one is given.
    let entry = listed(&paul, NEW).await.expect("account listed");
    assert_eq!(entry["status"], "active");
    assert_eq!(entry["assignments"], json!([]));
    assert_eq!(entry["orgUnitId"], Value::Null);
    assert_eq!(entry["clearance"], "internal");
    assert_eq!(zoe.get("/admin/users").await.status, StatusCode::FORBIDDEN);
    // Without a password.
    let refused = api.try_login(NEW, "un-mot-de-passe-quelconque").await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    let account: Uuid = entry["id"].as_str().unwrap().parse().unwrap();
    let created = creations(&api, since).await;
    assert_eq!(
        created,
        vec![(Some(account), json!({ "method": "sso", "providerId": id }))]
    );

    // Signed in again, by the link or by the address: the same account.
    let since = latest_entry(&api).await;
    let again = sign_in_by_link(&api, &idp, &id).await;
    assert_eq!(location(&again), "/chat");
    assert!(creations(&api, since).await.is_empty());
    let logins = sso_logins(&api, tenant::ID, since).await;
    assert_eq!(logins, vec![("success".into(), Some(account), None)]);
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn no_account_is_created_unless_the_provider_says_so() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    assert_eq!(try_link(&api, &id).await.json()["createsAccounts"], false);
    let other = "yanis.autre@neo-maintenance.example";
    let named = json!({ "given_name": "Yanis", "family_name": "Autre" });
    idp.sign_in_as(person("kc-yanis", other, named));

    let since = latest_entry(&api).await;
    assert_refused(&sign_in_by_link(&api, &idp, &id).await);
    let logins = sso_logins(&api, tenant::ID, since).await;
    assert_eq!(
        logins[0].2.as_deref(),
        Some("Aucun compte de l’entreprise pour cette adresse")
    );
    assert_eq!(listed(&paul, other).await, None);

    // Said so: the account, named by its given and family names.
    let path = format!("{PROVIDERS}/{id}");
    let changed = paul.patch(&path, json!({ "createAccounts": true })).await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let listed_provider = paul.get(PROVIDERS).await.json();
    assert_eq!(listed_provider["providers"][0]["createAccounts"], true);
    assert_eq!(location(&sign_in_by_link(&api, &idp, &id).await), "/chat");
    let entry = listed(&paul, other).await.expect("account listed");
    assert_eq!(entry["displayName"], "Yanis Autre");
    let changes = provider_changes(&api, since).await;
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0]["createAccounts"], true);
    assert_eq!(changes[0]["previousCreateAccounts"], false);
    remove(&paul, &id).await;
}

/// The changes of providers audited since `since`: their metadata.
async fn provider_changes(api: &TestApi, since: i64) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select metadata from audit_log where id > $1 \
         and action = 'identity_provider.updated' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_link_never_creates_an_account_it_must_not() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, creating(&idp)).await;
    // Another company's account with an address of Néo's domain.
    let theirs = "jeanne@neo-maintenance.example";
    let elsewhere = create(&api, "voisine", theirs).await;

    let cases = [
        (
            Person::verified("kc-hors", "hors@ailleurs.example"),
            "Adresse hors des domaines du fournisseur",
        ),
        (
            Person {
                email_verified: Some(false),
                ..Person::verified("kc-douteux", "douteux@neo-maintenance.example")
            },
            "Adresse e-mail non garantie par le fournisseur",
        ),
        (
            Person::verified("kc-jeanne", theirs),
            "Adresse déjà utilisée par un autre compte",
        ),
    ];
    for (who, reason) in cases {
        let address = who.email.clone().unwrap();
        idp.sign_in_as(who);
        let since = latest_entry(&api).await;
        assert_refused(&sign_in_by_link(&api, &idp, &id).await);
        let logins = sso_logins(&api, tenant::ID, since).await;
        assert_eq!(logins[0].2.as_deref(), Some(reason), "{address}");
        assert!(creations(&api, since).await.is_empty(), "{address}");
        assert_eq!(listed(&paul, &address).await, None, "{address}");
    }
    // The other company's account is left as it was: not tied, theirs.
    let mut tx = api
        .app
        .services
        .db
        .begin(elsewhere.tenant_id)
        .await
        .unwrap();
    let ties: i64 = sqlx::query_scalar("select count(*) from user_identities")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(ties, 0);
    drop(tx);
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_names_only_an_enabled_provider_of_an_active_company() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, creating(&idp)).await;
    let unknown = |response: support::TestResponse| {
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "{}",
            response.text()
        );
        assert!(
            response
                .text()
                .contains("Lien de connexion inconnu ou désactivé")
        );
    };
    unknown(try_link(&api, &Uuid::new_v4().to_string()).await);
    unknown(try_start_link(&api, &Uuid::new_v4().to_string()).await);
    let malformed = try_link(&api, "pas-un-identifiant").await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);

    let path = format!("{PROVIDERS}/{id}");
    paul.patch(&path, json!({ "enabled": false })).await;
    unknown(try_link(&api, &id).await);
    unknown(try_start_link(&api, &id).await);
    paul.patch(&path, json!({ "enabled": true })).await;

    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let status = "update tenants set status = $2 where id = $1";
    let set = |value: &'static str| {
        sqlx::query(status)
            .bind(tenant::ID)
            .bind(value)
            .execute(&owner)
    };
    set("suspended").await.unwrap();
    let suspended = (try_link(&api, &id).await, try_start_link(&api, &id).await);
    set("active").await.unwrap();
    unknown(suspended.0);
    unknown(suspended.1);
    assert_eq!(try_link(&api, &id).await.status, StatusCode::OK);
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_link_is_read_and_started_so_many_times_a_minute() {
    let api = TestApi::start_with(&[
        SSO_DEPLOYMENT[0],
        SSO_DEPLOYMENT[1],
        ("LOGIN_RATE_LIMIT_PER_MINUTE", "2"),
    ])
    .await;
    let idp = MockIdentityProvider::start().await;
    let id = Uuid::new_v4().to_string();
    // As an invitation's link is read: the login limit.
    for _ in 0..2 {
        assert_eq!(try_link(&api, &id).await.status, StatusCode::NOT_FOUND);
    }
    let refused = try_link(&api, &id).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    // Started as a sign-in is: twice the login limit, the return included.
    for _ in 0..4 {
        assert_eq!(
            try_start_link(&api, &id).await.status,
            StatusCode::NOT_FOUND
        );
    }
    let refused = try_start_link(&api, &id).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    drop(idp);
}
