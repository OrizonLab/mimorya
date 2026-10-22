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
use support::refusals::latest_entry;
use support::saml_provider::{MockSamlProvider, SAML_ENTITY_ID, SAML_SIGN_IN_URL, SamlPerson};
use support::sso::{
    NADIA_OID, PROVIDERS, SSO_DEPLOYMENT, assert_saml_refused, come_back, declare, identities,
    location, post_saml, remove, saml_attempt, saml_declaration, saml_nadia, saml_session,
    saml_sp_metadata, sso_logins, sso_refusal, try_start,
};
use support::{TestApi, WEB_ORIGIN, email};

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
    let response = idp.respond(&url, &saml_sp_metadata(&api).await, &saml_nadia());
    let back = post_saml(&api, &response, Some(&cookie)).await;
    assert_eq!(location(&back), "/chat");
    let token = saml_session(&back).expect("session");
    let me = api.as_user(&token).get("/auth/me").await.json();
    assert_eq!(me["user"]["email"], nadia_email.as_str());
    // Tied by the object ID, which outlives a change of its NameID.
    assert!(
        identities(&api)
            .await
            .contains(&(user::NADIA, NADIA_OID.into()))
    );
    let logins = sso_logins(&api, tenant::ID, since).await;
    assert_eq!(logins, vec![("success".into(), Some(user::NADIA), None)]);

    // The same response again, attempt and all: refused.
    let since = latest_entry(&api).await;
    let replayed = post_saml(&api, &response, Some(&cookie)).await;
    assert_saml_refused(&replayed);
    assert_eq!(
        sso_refusal(&api, since).await,
        "Assertion SAML déjà utilisée"
    );

    // Another sign-in: the account tied.
    let (url, cookie) = saml_attempt(&try_start(&api, &nadia_email).await);
    let response = idp.respond(&url, &sp, &saml_nadia());
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
    let response = idp.respond(&url, &sp, &saml_nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, None).await);
    assert!(sso_logins(&api, tenant::ID, since).await.is_empty());
    let (_, cookie) = start().await;
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, "", Some(&cookie)).await);
    assert!(
        sso_refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // For another request than this attempt's.
    let (first, _) = start().await;
    let (_, second) = start().await;
    let response = idp.respond(&first, &sp, &saml_nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&second)).await);
    assert!(
        sso_refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // Signed by a key the metadata does not declare.
    let (url, cookie) = start().await;
    let forged = MockSamlProvider::impostor().respond(&url, &sp, &saml_nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &forged, Some(&cookie)).await);
    assert!(
        sso_refusal(&api, since)
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
        sso_refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // Only the response signed, not its assertion.
    let (url, cookie) = start().await;
    let unsigned_sp = sp.replace(
        r#"WantAssertionsSigned="true""#,
        r#"WantAssertionsSigned="false""#,
    );
    let response = idp.respond(&url, &unsigned_sp, &saml_nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    assert!(
        sso_refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // Signed by the provider's own key, but outside the strict profile
    // (an inclusive canonicalisation).
    let (url, cookie) = start().await;
    let lax = MockSamlProvider::lax().respond(&url, &sp, &saml_nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &lax, Some(&cookie)).await);
    assert!(
        sso_refusal(&api, since)
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
    let response = idp.respond_to(&url, &sp, &elsewhere, &saml_nadia());
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    assert!(
        sso_refusal(&api, since)
            .await
            .starts_with("SAML : réponse refusée")
    );

    // A SAML attempt brought to the OpenID Connect return.
    let (_, cookie) = start().await;
    let sealed = cookie.replacen("mimorya_saml=", "mimorya_sso=", 1);
    let since = latest_entry(&api).await;
    let crossed = come_back(&api, "/auth/sso/callback?state=&code=x", Some(&sealed)).await;
    assert_eq!(location(&crossed), "/login?sso=echec");
    assert_eq!(sso_refusal(&api, since).await, "Tentative SAML");

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
        sso_refusal(&api, since)
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
        sso_refusal(&api, since).await,
        "Adresse hors des domaines du fournisseur"
    );
    // Disabled meanwhile.
    let (url, cookie) = start().await;
    let response = idp.respond(&url, &sp, &saml_nadia());
    let path = format!("{PROVIDERS}/{id}");
    paul.patch(&path, json!({ "enabled": false })).await;
    let since = latest_entry(&api).await;
    assert_saml_refused(&post_saml(&api, &response, Some(&cookie)).await);
    assert_eq!(
        sso_refusal(&api, since).await,
        "Fournisseur retiré ou désactivé"
    );
    // Nothing of it was tied.
    assert!(identities(&api).await.is_empty());
    remove(&paul, &id).await;
}
