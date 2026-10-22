//! Declaring a SAML 2.0 provider (ADR-068): its metadata read and
//! checked, its certificates taken anew from the same entity, accounts
//! created through the company's link, and its trust bound to the company
//! whose own entity ID it was given.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies::create;
use support::refusals::latest_entry;
use support::saml_provider::{
    DISPLAY_NAME, MockSamlProvider, OBJECT_ID, SAML_ENTITY_ID, SAML_SIGN_IN_URL, SamlPerson,
};
use support::sso::{
    PROVIDERS, SSO_DEPLOYMENT, assert_saml_refused, declare, location, post_saml, prove_domain,
    remove, saml_attempt, saml_declaration, saml_nadia, saml_session, saml_sp_metadata,
    saml_sp_metadata_of, sso_logins, sso_refusal, try_start, try_start_link,
};
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_declaration_reads_the_metadata_and_takes_its_certificates_anew() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockSamlProvider::new();
    let paul = api.sign_in(user::PAUL).await;
    let metadata = idp.metadata();
    for (body, message) in [
        (
            json!({ "kind": "saml", "name": "SAML", "domains": ["neo-maintenance.example"] }),
            "collez les métadonnées",
        ),
        (saml_declaration("pas du XML"), "métadonnées illisibles"),
        (
            saml_declaration(&metadata.replace("HTTP-Redirect", "HTTP-POST")),
            "sans adresse de connexion",
        ),
        (
            {
                let mut body = saml_declaration(&metadata);
                body["clientSecret"] = json!("secret");
                body
            },
            "sans émetteur, client ni secret",
        ),
        (
            json!({
                "kind": "oidc", "name": "OIDC", "issuer": "https://id.example.com",
                "clientId": "c", "clientSecret": "s", "metadata": metadata,
                "domains": ["neo-maintenance.example"],
            }),
            "pour un fournisseur SAML seulement",
        ),
    ] {
        let refused = paul.post(PROVIDERS, body).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        assert!(
            refused.error_message().contains(message),
            "{}",
            refused.text()
        );
    }

    let id = declare(&paul, saml_declaration(&metadata)).await;
    let path = format!("{PROVIDERS}/{id}");
    // Its certificate rolled over: the new metadata, the same provider.
    let renewed = MockSamlProvider::impostor();
    let since = latest_entry(&api).await;
    let changed = paul
        .patch(&path, json!({ "metadata": renewed.metadata() }))
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let sp = saml_sp_metadata(&api).await;
    let (url, cookie) = saml_attempt(&try_start(&api, &email(user::NADIA)).await);
    let old = idp.respond(&url, &sp, &saml_nadia());
    assert_saml_refused(&post_saml(&api, &old, Some(&cookie)).await);
    let (url, cookie) = saml_attempt(&try_start(&api, &email(user::NADIA)).await);
    let new = renewed.respond(&url, &sp, &saml_nadia());
    assert_eq!(
        location(&post_saml(&api, &new, Some(&cookie)).await),
        "/chat"
    );
    let _ = since;
    // Never another provider's, nor a secret.
    let other = metadata.replace(SAML_ENTITY_ID, "https://autre.saml.example/metadata");
    let refused = paul.patch(&path, json!({ "metadata": other })).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused
            .error_message()
            .contains("celles d’un autre fournisseur")
    );
    let secret = paul.patch(&path, json!({ "clientSecret": "secret" })).await;
    assert_eq!(secret.status, StatusCode::BAD_REQUEST, "{}", secret.text());
    remove(&paul, &id).await;

    // A sign-in address on the private network, on a server that does not
    // allow it.
    let public = TestApi::start_with(&[SSO_DEPLOYMENT[0]]).await;
    let admin = public.sign_in(user::PAUL).await;
    let private = metadata.replace(SAML_SIGN_IN_URL, "http://10.0.0.7/sso");
    let refused = admin.post(PROVIDERS, saml_declaration(&private)).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(refused.error_message().contains("adresse de connexion"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_link_creates_the_account_through_a_saml_provider() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockSamlProvider::new();
    let paul = api.sign_in(user::PAUL).await;
    let mut body = saml_declaration(&idp.metadata());
    body["createAccounts"] = json!(true);
    let id = declare(&paul, body).await;
    prove_domain(&api, &paul, tenant::ID, "neo-maintenance.example").await;
    let sp = saml_sp_metadata(&api).await;
    let newcomer = "lina.saml@neo-maintenance.example";
    let person = SamlPerson::named(newcomer)
        .with(OBJECT_ID, "00000000-0000-0000-0000-0000000000bb")
        .with(DISPLAY_NAME, "Lina Saml");

    // From an address: nothing created.
    let (url, cookie) = saml_attempt(&try_start(&api, &email(user::NADIA)).await);
    let response = idp.respond(&url, &sp, &person);
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    assert_eq!(
        sso_refusal(&api, since).await,
        "Aucun compte de l’entreprise pour cette adresse"
    );

    // From the company's link: the account, named by the provider.
    let (url, cookie) = saml_attempt(&try_start_link(&api, &id).await);
    let response = idp.respond(&url, &sp, &person);
    let back = post_saml(&api, &response, Some(&cookie)).await;
    assert_eq!(location(&back), "/chat");
    let token = saml_session(&back).expect("session");
    let me = api.as_user(&token).get("/auth/me").await.json();
    assert_eq!(me["user"]["email"], newcomer);
    assert_eq!(me["user"]["displayName"], "Lina Saml");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_provider_s_trust_is_bound_to_one_company() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockSamlProvider::new();
    // Another company declares Néo's provider, for Néo's domain.
    let admin = "admin@voisine-saml.example";
    let other = create(&api, "voisine-saml", admin).await;
    let accepted = accept(&api, &other.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let theirs = api.sign_in_with(admin, INVITED_PASSWORD).await;
    let mut body = saml_declaration(&idp.metadata());
    body["createAccounts"] = json!(true);
    let id = declare(&theirs, body).await;
    // Their entity ID is theirs, not Néo's.
    let neo = saml_sp_metadata(&api).await;
    let their_sp = saml_sp_metadata_of(&api, other.tenant_id).await;
    assert_ne!(neo, their_sp);

    // Néo's provider trusts Néo alone: what it says for Néo is refused
    // there, and opens nothing.
    let (url, cookie) = saml_attempt(&try_start_link(&api, &id).await);
    let response = idp.respond_to(&url, &their_sp, &neo, &saml_nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    let logins = sso_logins(&api, other.tenant_id, since).await;
    assert_eq!(logins.len(), 1, "{logins:?}");
    assert!(
        logins[0]
            .2
            .as_deref()
            .unwrap_or_default()
            .starts_with("SAML : réponse refusée")
    );
    remove(&theirs, &id).await;
}
