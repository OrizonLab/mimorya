//! Signing in through a SAML 2.0 provider (ADR-068): declared from its
//! metadata, it receives the browser with a request and posts back a
//! signed response, which ties the account by its stable identifier and
//! signs it in. A response replayed, signed by another key, altered, for
//! another request or without the attempt is refused, audited, and opens
//! no session.

mod support;

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies::create;
use support::refusals::latest_entry;
use support::saml_provider::{
    DISPLAY_NAME, MockSamlProvider, OBJECT_ID, SAML_ENTITY_ID, SAML_SIGN_IN_URL, SamlPerson,
};
use support::sso::{
    PROVIDERS, SSO_DEPLOYMENT, assert_saml_refused, come_back, declare, identities, location,
    post_saml, remove, saml_attempt, saml_declaration, saml_session, saml_sp_metadata,
    saml_sp_metadata_of, sso_logins, try_start, try_start_link,
};
use support::{TestApi, WEB_ORIGIN, email};

const OID: &str = "00000000-0000-0000-0000-00000000000a";

/// Nadia, as Entra ID sends her: her sign-in name, her object ID.
fn nadia() -> SamlPerson {
    SamlPerson::named(&email(user::NADIA))
        .with(OBJECT_ID, OID)
        .with(DISPLAY_NAME, "Nadia Benali")
}

/// The reason of the single refusal audited since `since`.
async fn refusal(api: &TestApi, since: i64) -> String {
    let logins = sso_logins(api, tenant::ID, since).await;
    assert_eq!(logins.len(), 1, "{logins:?}");
    assert_eq!(logins[0].0, "denied");
    logins[0].2.clone().unwrap_or_default()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_saml_provider_signs_its_domain_s_accounts_in() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockSamlProvider::new();
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, saml_declaration(&idp.metadata())).await;

    // Declared from its metadata: no client secret; the address to give
    // the provider listed.
    let listed = paul.get(PROVIDERS).await.json();
    let entity = format!("{WEB_ORIGIN}/api/auth/saml/{}/metadata", tenant::ID);
    assert_eq!(listed["samlEntityId"], entity);
    assert_eq!(
        listed["samlAcsUrl"],
        format!("{WEB_ORIGIN}/api/auth/saml/acs")
    );
    let provider = &listed["providers"][0];
    assert_eq!(provider["kind"], "saml");
    assert_eq!(provider["issuer"], SAML_ENTITY_ID);
    assert_eq!(provider["clientId"], entity);
    assert_eq!(provider["secretStored"], false);
    let sp = saml_sp_metadata(&api).await;
    assert!(
        sp.contains(&entity) && sp.contains("/api/auth/saml/acs"),
        "{sp}"
    );
    assert!(sp.contains(r#"WantAssertionsSigned="true""#), "{sp}");
    let malformed = api.anonymous().get("/auth/saml/pas-un-id/metadata").await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);

    // Offered from the address, sent with a request, back signed in.
    let nadia_email = email(user::NADIA);
    let since = latest_entry(&api).await;
    let (url, cookie) = saml_attempt(&try_start(&api, &nadia_email).await);
    assert!(
        url.starts_with(SAML_SIGN_IN_URL) && url.contains("SAMLRequest="),
        "{url}"
    );
    let response = idp.respond(&url, &saml_sp_metadata(&api).await, &nadia());
    let back = post_saml(&api, &response, Some(&cookie)).await;
    assert_eq!(location(&back), "/chat");
    let token = saml_session(&back).expect("session");
    let me = api.as_user(&token).get("/auth/me").await.json();
    assert_eq!(me["user"]["email"], nadia_email.as_str());
    // Tied by the object ID, which outlives a change of its NameID.
    assert!(identities(&api).await.contains(&(user::NADIA, OID.into())));
    let logins = sso_logins(&api, tenant::ID, since).await;
    assert_eq!(logins, vec![("success".into(), Some(user::NADIA), None)]);

    // The same response again, attempt and all: refused.
    let since = latest_entry(&api).await;
    let replayed = post_saml(&api, &response, Some(&cookie)).await;
    assert_saml_refused(&replayed);
    assert_eq!(refusal(&api, since).await, "Assertion SAML déjà utilisée");

    // Another sign-in: the account tied.
    let (url, cookie) = saml_attempt(&try_start(&api, &nadia_email).await);
    let response = idp.respond(&url, &sp, &nadia());
    let again = post_saml(&api, &response, Some(&cookie)).await;
    assert_eq!(location(&again), "/chat");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_response_that_does_not_answer_this_attempt_is_refused() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockSamlProvider::new();
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, saml_declaration(&idp.metadata())).await;
    let sp = saml_sp_metadata(&api).await;
    let nadia_email = email(user::NADIA);
    let start = || async { saml_attempt(&try_start(&api, &nadia_email).await) };

    // Without the attempt, or with nothing posted.
    let (url, _) = start().await;
    let response = idp.respond(&url, &sp, &nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, None).await);
    assert!(sso_logins(&api, tenant::ID, since).await.is_empty());
    let (_, cookie) = start().await;
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, "", Some(&cookie)).await);
    assert!(
        refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // For another request than this attempt's.
    let (first, _) = start().await;
    let (_, second) = start().await;
    let response = idp.respond(&first, &sp, &nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&second)).await);
    assert!(
        refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // Signed by a key the metadata does not declare.
    let (url, cookie) = start().await;
    let forged = MockSamlProvider::impostor().respond(&url, &sp, &nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &forged, Some(&cookie)).await);
    assert!(
        refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // Altered after signing: Paul instead of Nadia.
    let (url, cookie) = start().await;
    let signed = idp.respond(&url, &sp, &SamlPerson::named(&nadia_email));
    let xml = String::from_utf8(STANDARD.decode(&signed).unwrap()).unwrap();
    assert!(xml.contains(&nadia_email));
    let altered = STANDARD.encode(xml.replace(&nadia_email, &email(user::PAUL)));
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &altered, Some(&cookie)).await);
    assert!(
        refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // Only the response signed, not its assertion.
    let (url, cookie) = start().await;
    let unsigned_sp = sp.replace(
        r#"WantAssertionsSigned="true""#,
        r#"WantAssertionsSigned="false""#,
    );
    let response = idp.respond(&url, &unsigned_sp, &nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    assert!(
        refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // Signed by the provider's own key, but outside the strict profile
    // (an inclusive canonicalisation).
    let (url, cookie) = start().await;
    let lax = MockSamlProvider::lax().respond(&url, &sp, &nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &lax, Some(&cookie)).await);
    assert!(
        refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // For another application (its audience).
    let (url, cookie) = start().await;
    let elsewhere = sp.replace(
        &format!(
            r#"entityID="{WEB_ORIGIN}/api/auth/saml/{}/metadata""#,
            tenant::ID
        ),
        r#"entityID="https://autre-application.example/saml""#,
    );
    assert_ne!(elsewhere, sp);
    let response = idp.respond_to(&url, &sp, &elsewhere, &nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    assert!(
        refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // A SAML attempt brought to the OpenID Connect return.
    let (_, cookie) = start().await;
    let sealed = cookie.replacen("mimorya_saml=", "mimorya_sso=", 1);
    let since = latest_entry(&api).await;
    let crossed = come_back(&api, "/auth/sso/callback?state=&code=x", Some(&sealed)).await;
    assert_eq!(location(&crossed), "/login?sso=echec");
    assert_eq!(refusal(&api, since).await, "Tentative SAML");

    // Wrapped: a forged assertion for Paul set before Nadia's signed one.
    let (url, cookie) = start().await;
    let signed = idp.respond(&url, &sp, &SamlPerson::named(&nadia_email));
    let xml = String::from_utf8(STANDARD.decode(&signed).unwrap()).unwrap();
    let (open, close) = ("<saml:Assertion", "</saml:Assertion>");
    let begin = xml.find(open).expect("assertion");
    let end = xml.find(close).expect("assertion end") + close.len();
    let original = &xml[begin..end];
    let signature = original.find("<ds:Signature").expect("signature");
    let signature_end = original.find("</ds:Signature>").expect("signature end");
    let unsigned = format!(
        "{}{}",
        &original[..signature],
        &original[signature_end + "</ds:Signature>".len()..]
    );
    let forged = unsigned
        .replacen(r#"ID=""#, r#"ID="_forged"#, 1)
        .replace(&nadia_email, &email(user::PAUL));
    let wrapped = format!("{}{forged}{}", &xml[..begin], &xml[begin..]);
    let since = latest_entry(&api).await;
    let posted = post_saml(&api, &STANDARD.encode(wrapped), Some(&cookie)).await;
    assert_saml_refused(&posted);
    assert!(
        refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // Signed, but for an address outside the provider's domains.
    let (url, cookie) = start().await;
    let outsider = SamlPerson::named("nadia@ailleurs.example");
    let response = idp.respond(&url, &sp, &outsider);
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    assert_eq!(
        refusal(&api, since).await,
        "Adresse hors des domaines du fournisseur"
    );
    // Disabled meanwhile.
    let (url, cookie) = start().await;
    let response = idp.respond(&url, &sp, &nadia());
    let path = format!("{PROVIDERS}/{id}");
    paul.patch(&path, json!({ "enabled": false })).await;
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    assert_eq!(
        refusal(&api, since).await,
        "Fournisseur retiré ou désactivé"
    );
    // Nothing of it was tied.
    assert!(identities(&api).await.is_empty());
    remove(&paul, &id).await;
}

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
    let old = idp.respond(&url, &sp, &nadia());
    assert_saml_refused(&post_saml(&api, &old, Some(&cookie)).await);
    let (url, cookie) = saml_attempt(&try_start(&api, &email(user::NADIA)).await);
    let new = renewed.respond(&url, &sp, &nadia());
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
        refusal(&api, since).await,
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
    let response = idp.respond_to(&url, &their_sp, &neo, &nadia());
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
