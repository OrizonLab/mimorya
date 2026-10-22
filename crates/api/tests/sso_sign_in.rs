//! Signing in through the company's identity provider (ADR-063): offered
//! only to the accounts of a company covering their domain; the first
//! sign-in ties the account to the person's subject at the provider, by
//! the address it vouches for, and later ones go by the subject alone; the
//! session is the one a password opens; Microsoft's directories are
//! read by the object ID.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::identity_providers::{
    NewIdentityProvider, create_identity_provider, replace_provider_domains,
};
use serde_json::{Map, json};
use support::identity_provider::{CLIENT_ID, CLIENT_SECRET, MockIdentityProvider, Person};
use support::refusals::latest_entry;
use support::sso::{
    DOMAIN, SSO_DEPLOYMENT, come_back, declaration, declare, discover, identities, location,
    remove, session, sign_in, sso_logins, start,
};
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_signs_in_through_its_company_s_provider_tied_once() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    assert_eq!(discover(&api, &nadia).await, json!(null));
    let id = declare(&paul, declaration(&idp)).await;

    // Offered to the company's accounts of the domain, whatever the case;
    // an address without an account reads as one without a provider.
    let offered = json!({ "kind": "oidc", "name": "Annuaire Néo", "required": false });
    assert_eq!(discover(&api, &nadia).await, offered);
    assert_eq!(discover(&api, &nadia.to_uppercase()).await, offered);
    assert_eq!(
        discover(&api, &format!("personne@{DOMAIN}")).await,
        json!(null)
    );

    let since = latest_entry(&api).await;
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    let back = sign_in(&api, &idp, &nadia).await;
    assert_eq!(location(&back), "/chat");
    let token = session(&back).expect("session");
    let me = api.as_user(&token).get("/auth/me").await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["user"]["id"], json!(user::NADIA));
    assert_eq!(
        identities(&api).await,
        [(user::NADIA, "kc-nadia".to_string())]
    );
    assert_eq!(
        sso_logins(&api, tenant::ID, since).await,
        [("success".to_string(), Some(user::NADIA), None)]
    );

    // Tied: the subject alone counts, whatever the address says now.
    idp.sign_in_as(Person {
        subject: "kc-nadia".into(),
        email: Some("autre.adresse@ailleurs.example".into()),
        email_verified: Some(false),
        ..Person::default()
    });
    let again = sign_in(&api, &idp, &nadia).await;
    assert_eq!(location(&again), "/chat");
    let token = session(&again).expect("session");
    let me = api.as_user(&token).get("/auth/me").await.json();
    assert_eq!(me["user"]["id"], json!(user::NADIA));
    assert_eq!(identities(&api).await.len(), 1);

    // Whom the provider proves signs in, not whom the address named.
    let marc = email(user::MARC);
    idp.sign_in_as(Person::verified("kc-marc", &marc));
    let other = sign_in(&api, &idp, &nadia).await;
    let token = session(&other).expect("session");
    let me = api.as_user(&token).get("/auth/me").await.json();
    assert_eq!(me["user"]["id"], json!(user::MARC));

    // Removed, the provider takes its ties with it.
    remove(&paul, &id).await;
    assert!(identities(&api).await.is_empty());
    assert_eq!(discover(&api, &nadia).await, json!(null));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_provider_is_neither_offered_nor_honoured() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    let id = declare(&paul, declaration(&idp)).await;
    let path = format!("/admin/identity-providers/{id}");

    // Started, then disabled before the return.
    let (url, cookie) = start(&api, &nadia).await;
    let disabled = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(discover(&api, &nadia).await, json!(null));
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    let since = latest_entry(&api).await;
    let back = come_back(&api, &idp.authorize(&url), Some(&cookie)).await;
    support::sso::assert_refused(&back);
    assert_eq!(idp.exchanges(), 0);
    assert_eq!(
        sso_logins(&api, tenant::ID, since).await,
        [(
            "denied".to_string(),
            None,
            Some("Fournisseur retiré ou désactivé".to_string())
        )]
    );
    let refused = support::sso::try_start(&api, &nadia).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());

    let enabled = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    remove(&paul, &id).await;
}

/// A Microsoft Entra ID directory is reached at Microsoft: its sign-in
/// rules are played by the local provider under Microsoft's kind.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_microsoft_directory_s_account_is_known_by_its_object_id() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let services = &api.app.services;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let id = create_identity_provider(
        &mut tx,
        NewIdentityProvider {
            tenant_id: tenant::ID,
            kind: "microsoft",
            name: "Microsoft 365",
            issuer: &idp.issuer,
            client_id: CLIENT_ID,
            enabled: true,
            required: false,
            create_accounts: false,
            saml_metadata: None,
            created_by_user_id: user::PAUL,
        },
    )
    .await
    .unwrap()
    .unwrap();
    let domains = [DOMAIN.to_string()];
    replace_provider_domains(&mut tx, tenant::ID, id, &domains)
        .await
        .unwrap();
    let secret_name = mimorya_api::services::sso::client_secret_name(id);
    mimorya_api::secrets::SecretStore::new(services.config.secrets.clone())
        .put(&mut tx, tenant::ID, &secret_name, CLIENT_SECRET)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let nadia = email(user::NADIA);
    let offered = json!({ "kind": "microsoft", "name": "Microsoft 365", "required": false });
    assert_eq!(discover(&api, &nadia).await, offered);
    // No `email` claim: the sign-in name of the directory's account; the
    // object ID, not the pairwise subject, ties it.
    let oid = "00000000-0000-0000-0000-00000000a5a5";
    let mut extra = Map::new();
    extra.insert("oid".into(), json!(oid));
    let directory_account = Person {
        subject: "sujet-propre-a-l-application".into(),
        preferred_username: Some(nadia.to_uppercase()),
        extra,
        ..Person::default()
    };
    idp.sign_in_as(directory_account.clone());
    let back = sign_in(&api, &idp, &nadia).await;
    assert_eq!(location(&back), "/chat");
    assert!(session(&back).is_some());
    assert_eq!(identities(&api).await, [(user::NADIA, oid.to_string())]);

    // Another application of the directory: another subject, same object.
    idp.sign_in_as(Person {
        subject: "autre-sujet".into(),
        ..directory_account
    });
    let again = sign_in(&api, &idp, &nadia).await;
    assert_eq!(location(&again), "/chat");
    assert_eq!(identities(&api).await.len(), 1);

    // Without its object ID, a Microsoft token proves nobody.
    idp.sign_in_as(Person::verified("sans-oid", &nadia));
    support::sso::assert_refused(&sign_in(&api, &idp, &nadia).await);
    remove(&paul, &id.to_string()).await;
}
