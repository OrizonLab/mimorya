//! A company's identity provider serves only that company's accounts
//! (ADR-063), even for an address of the same domain in another company,
//! active or invited there (ADR-064);
//! another company may declare the same domain for its own; an attempt
//! opens only in its company; a suspended company, or a server without its
//! master key, signs nobody in this way.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies::create;
use support::identity_provider::{MockIdentityProvider, Person};
use support::llm::MASTER_KEY;
use support::refusals::{latest_entry, latest_entry_of};
use support::sso::{
    PROVIDERS, SSO_DEPLOYMENT, assert_refused, come_back, declaration, declare, discover,
    identities, location, remove, session, sign_in, sso_logins, start, try_start,
};
use support::{TestApi, email, owner_url};

const INTRUS: &str = "intrus@neo-maintenance.example";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_provider_serves_only_its_own_company_s_accounts() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    let neo = declare(&paul, declaration(&idp)).await;
    // Another company, whose administrator's address is of Néo's domain.
    let other = create(&api, "intrus", INTRUS).await;
    let accepted = accept(&api, &other.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let intrus = api.sign_in_with(INTRUS, INVITED_PASSWORD).await;

    // Néo's provider is not offered to it, and does not sign it in: Néo's
    // accounts alone are looked up.
    assert_eq!(discover(&api, INTRUS).await, json!(null));
    let since = latest_entry(&api).await;
    let since_other = latest_entry_of(&api, other.tenant_id).await;
    idp.sign_in_as(Person::verified("kc-intrus", INTRUS));
    assert_refused(&sign_in(&api, &idp, &nadia).await);
    assert_eq!(
        sso_logins(&api, tenant::ID, since).await,
        [(
            "denied".to_string(),
            None,
            Some("Aucun compte de l’entreprise pour cette adresse".to_string())
        )]
    );
    assert!(
        sso_logins(&api, other.tenant_id, since_other)
            .await
            .is_empty()
    );

    // The other company declares the same domain for its own accounts.
    let theirs = declare(&intrus, declaration(&idp)).await;
    assert_eq!(discover(&api, INTRUS).await["name"], "Annuaire Néo");
    let back = sign_in(&api, &idp, INTRUS).await;
    let token = session(&back).expect("session");
    let me = api.as_user(&token).get("/auth/me").await.json();
    assert_eq!(me["tenant"]["id"], json!(other.tenant_id));
    // Through it, Néo's accounts are nobody.
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    assert_refused(&sign_in(&api, &idp, INTRUS).await);
    assert!(identities(&api).await.is_empty());

    // An attempt opens only in its own company.
    let (url, cookie) = start(&api, &nadia).await;
    let moved = cookie.replacen(&tenant::ID.to_string(), &other.tenant_id.to_string(), 1);
    assert_ne!(moved, cookie);
    let since_other = latest_entry_of(&api, other.tenant_id).await;
    assert_refused(&come_back(&api, &idp.authorize(&url), Some(&moved)).await);
    assert!(
        sso_logins(&api, other.tenant_id, since_other)
            .await
            .is_empty()
    );

    // Each administrator sees and changes only their company's providers.
    let listed = paul.get(PROVIDERS).await.json()["providers"].clone();
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
    let path = format!("{PROVIDERS}/{theirs}");
    let changed = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(changed.status, StatusCode::NOT_FOUND, "{}", changed.text());
    assert_eq!(paul.delete(&path).await.status, StatusCode::NOT_FOUND);
    assert_eq!(discover(&api, INTRUS).await["name"], "Annuaire Néo");

    remove(&intrus, &theirs).await;
    remove(&paul, &neo).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_invited_account_is_never_accepted_here() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let neo = declare(&paul, declaration(&idp)).await;
    // Still invited, its address of Néo's domain (ADR-064).
    let address = "invite.intrus@neo-maintenance.example";
    let other = create(&api, "intrus-invite", address).await;

    assert_eq!(discover(&api, address).await, json!(null));
    let since_other = latest_entry_of(&api, other.tenant_id).await;
    idp.sign_in_as(Person::verified("kc-invite-intrus", address));
    assert_refused(&sign_in(&api, &idp, &email(user::NADIA)).await);
    assert!(
        sso_logins(&api, other.tenant_id, since_other)
            .await
            .is_empty()
    );
    // Untouched: still invited, its own link still serves.
    let accepted = accept(&api, &other.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    remove(&paul, &neo).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_suspended_company_signs_nobody_in_this_way() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    let id = declare(&paul, declaration(&idp)).await;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let set_status = |status: &'static str| {
        sqlx::query("update tenants set status = $2 where id = $1")
            .bind(tenant::ID)
            .bind(status)
            .execute(&owner)
    };

    let (url, cookie) = start(&api, &nadia).await;
    set_status("suspended").await.unwrap();
    assert_eq!(discover(&api, &nadia).await, json!(null));
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    let since = latest_entry(&api).await;
    assert_refused(&come_back(&api, &idp.authorize(&url), Some(&cookie)).await);
    assert_eq!(
        sso_logins(&api, tenant::ID, since).await,
        [(
            "denied".to_string(),
            None,
            Some("Entreprise suspendue".to_string())
        )]
    );
    set_status("active").await.unwrap();
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_its_master_key_a_server_signs_nobody_in_this_way() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let keyless = TestApi::start_with(&[("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true")]).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    let id = declare(&paul, declaration(&idp)).await;

    assert_eq!(discover(&keyless, &nadia).await, json!(null));
    let refused = try_start(&keyless, &nadia).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    let (url, cookie) = start(&api, &nadia).await;
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    let since = latest_entry(&api).await;
    assert_refused(&come_back(&keyless, &idp.authorize(&url), Some(&cookie)).await);
    assert!(sso_logins(&api, tenant::ID, since).await.is_empty());
    // Nor can a provider be declared there.
    let admin = keyless.sign_in(user::PAUL).await;
    let declared = admin.post(PROVIDERS, declaration(&idp)).await;
    assert_eq!(
        declared.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        declared.text()
    );
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_provider_out_of_the_network_policy_is_never_reached() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    // Same companies, but this server keeps to the public network.
    let public = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    let id = declare(&paul, declaration(&idp)).await;

    // Declared where the private network was allowed: here, its address is
    // refused before any connection, at the start as at the return.
    let before = idp.requests();
    let refused = try_start(&public, &nadia).await;
    assert_eq!(
        refused.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        refused.text()
    );
    assert_eq!(idp.requests(), before);
    // Started where it is allowed (its declaration kept there, ADR-065).
    let (url, cookie) = start(&api, &nadia).await;
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    let back = idp.authorize(&url);
    let since = latest_entry(&api).await;
    assert_refused(&come_back(&public, &back, Some(&cookie)).await);
    assert_eq!(idp.requests(), before);
    let audited = sso_logins(&api, tenant::ID, since).await;
    assert_eq!(audited.len(), 1, "{audited:?}");
    let reason = audited[0].2.clone().unwrap_or_default();
    assert!(reason.contains("https:// attendu"), "{reason}");
    remove(&paul, &id).await;
}
