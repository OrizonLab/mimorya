//! What a single sign-on return never opens a session for (ADR-063): no
//! attempt, another attempt's return, a provider's refusal, a code used
//! twice, a token for another client or another attempt; and every refusal
//! is audited in its company with its reason, never with a token or code.

mod support;

use mimorya_core::demo::{tenant, user};
use support::accounts::invite_account;
use support::identity_provider::{MockIdentityProvider, Person, Tamper};
use support::refusals::latest_entry;
use support::sso::{
    SSO_DEPLOYMENT, assert_refused, audit_text_since, come_back, declaration, declare, discover,
    identities, location, remove, session, sign_in, sso_logins, start, state_of, try_start,
};
use support::{TestApi, email};

/// The reasons of the refusals audited for the demo company since `since`.
async fn reasons(api: &TestApi, since: i64) -> Vec<String> {
    sso_logins(api, tenant::ID, since)
        .await
        .into_iter()
        .filter(|(outcome, _, _)| outcome == "denied")
        .map(|(_, _, reason)| reason.unwrap_or_default())
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_opens_a_session_only_for_its_own_attempt() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    let id = declare(&paul, declaration(&idp)).await;
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    let since = latest_entry(&api).await;

    // Without the attempt's cookie (another browser): nothing is known of
    // it, nothing is audited, the code is not even exchanged.
    let (url, _) = start(&api, &nadia).await;
    assert_refused(&come_back(&api, &idp.authorize(&url), None).await);
    let altered = "mimorya_sso=00000000-0000-0000-0000-000000000000.a.b.c";
    let (url, _) = start(&api, &nadia).await;
    assert_refused(&come_back(&api, &idp.authorize(&url), Some(altered)).await);
    assert!(reasons(&api, since).await.is_empty());

    // Another attempt's return, as a sign-in forced from elsewhere would be;
    // a refusal without the attempt's state is one too.
    let (mine, cookie) = start(&api, &nadia).await;
    let (theirs, _) = start(&api, &nadia).await;
    assert_refused(&come_back(&api, &idp.authorize(&theirs), Some(&cookie)).await);
    let forged = "/auth/sso/callback?error=access_denied";
    assert_refused(&come_back(&api, forged, Some(&cookie)).await);
    // The provider's own refusal, with the attempt's state.
    let refusal = format!(
        "/auth/sso/callback?error=access_denied&error_description=Annul%C3%A9&state={}",
        state_of(&mine)
    );
    assert_refused(&come_back(&api, &refusal, Some(&cookie)).await);
    assert_eq!(idp.exchanges(), 0);

    // The right return, once: replayed, its code is spent at the provider.
    let back = idp.authorize(&mine);
    let first = come_back(&api, &back, Some(&cookie)).await;
    assert_eq!(location(&first), "/chat");
    assert!(session(&first).is_some());
    assert_refused(&come_back(&api, &back, Some(&cookie)).await);

    assert_eq!(
        reasons(&api, since).await,
        [
            "Retour d’une autre tentative",
            "Retour d’une autre tentative",
            "Refusée par le fournisseur (access_denied)",
            "échange : refusé par le fournisseur (invalid_grant)",
        ]
    );
    // Neither the code, nor a token, nor the provider's words.
    let audited = audit_text_since(&api, tenant::ID, since).await;
    let code = back
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();
    for hidden in [code, "jeton-d-acces-de-test", "eyJ", "Annul"] {
        assert!(!audited.contains(hidden), "{hidden} in {audited}");
    }
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_token_for_another_client_or_attempt_proves_nobody() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    let id = declare(&paul, declaration(&idp)).await;
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    let since = latest_entry(&api).await;

    for tamper in [
        Tamper {
            audience: Some("une-autre-application".into()),
            ..Tamper::default()
        },
        Tamper {
            nonce: Some("nonce-d-une-autre-tentative".into()),
            ..Tamper::default()
        },
    ] {
        idp.tamper(tamper);
        assert_refused(&sign_in(&api, &idp, &nadia).await);
    }
    idp.tamper(Tamper::default());
    let reasons = reasons(&api, since).await;
    assert_eq!(reasons.len(), 2, "{reasons:?}");
    for reason in &reasons {
        assert!(reason.starts_with("jeton d’identité : "), "{reason}");
    }
    assert!(identities(&api).await.is_empty());
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_is_tied_only_by_an_address_the_provider_vouches_for() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = email(user::NADIA);
    let id = declare(&paul, declaration(&idp)).await;
    let since = latest_entry(&api).await;

    for person in [
        // An address the provider does not vouch for.
        Person {
            email_verified: Some(false),
            ..Person::verified("kc-1", &nadia)
        },
        Person {
            email_verified: None,
            ..Person::verified("kc-2", &nadia)
        },
        // Outside the provider's domains.
        Person::verified("kc-3", "nadia.benali@ailleurs.example"),
        // In the domain, but no account of the company has it.
        Person::verified("kc-4", "personne@neo-maintenance.example"),
    ] {
        idp.sign_in_as(person);
        assert_refused(&sign_in(&api, &idp, &nadia).await);
    }
    assert_eq!(
        reasons(&api, since).await,
        [
            "Adresse e-mail non garantie par le fournisseur",
            "Adresse e-mail non garantie par le fournisseur",
            "Adresse hors des domaines du fournisseur",
            "Aucun compte de l’entreprise pour cette adresse",
        ]
    );

    // Tied once, an account is not tied to another subject.
    idp.sign_in_as(Person::verified("kc-nadia", &nadia));
    assert_eq!(location(&sign_in(&api, &idp, &nadia).await), "/chat");
    let since = latest_entry(&api).await;
    idp.sign_in_as(Person::verified("kc-imposteur", &nadia));
    assert_refused(&sign_in(&api, &idp, &nadia).await);
    assert_eq!(
        sso_logins(&api, tenant::ID, since).await,
        [(
            "denied".to_string(),
            Some(user::NADIA),
            Some("Compte déjà relié à une autre identité de ce fournisseur".to_string())
        )]
    );
    assert_eq!(
        identities(&api).await,
        [(user::NADIA, "kc-nadia".to_string())]
    );
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_account_never_signs_in_nor_gets_tied() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = email(user::MARC);
    let id = declare(&paul, declaration(&idp)).await;
    let account = format!("/admin/users/{}", user::MARC);

    // Started while active, disabled before the return.
    let (url, cookie) = start(&api, &marc).await;
    let disabled = paul
        .patch(&account, serde_json::json!({ "status": "disabled" }))
        .await;
    assert_eq!(disabled.status, axum::http::StatusCode::OK);
    assert_eq!(discover(&api, &marc).await, serde_json::json!(null));
    idp.sign_in_as(Person::verified("kc-marc", &marc));
    let since = latest_entry(&api).await;
    assert_refused(&come_back(&api, &idp.authorize(&url), Some(&cookie)).await);
    assert_eq!(
        sso_logins(&api, tenant::ID, since).await,
        [(
            "denied".to_string(),
            Some(user::MARC),
            Some("Compte inactif".to_string())
        )]
    );
    // Its tie was not kept.
    assert!(identities(&api).await.is_empty());

    let enabled = paul
        .patch(&account, serde_json::json!({ "status": "active" }))
        .await;
    assert_eq!(enabled.status, axum::http::StatusCode::OK);
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_invited_account_is_offered_no_provider() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare(&paul, declaration(&idp)).await;
    // Its invitation is accepted first, with a password (ADR-063).
    let (invited, _) = invite_account(&paul, "invite.sso").await;
    assert_eq!(
        discover(&api, &invited.email).await,
        serde_json::json!(null)
    );
    let refused = try_start(&api, &invited.email).await;
    assert_eq!(
        refused.status,
        axum::http::StatusCode::NOT_FOUND,
        "{}",
        refused.text()
    );
    remove(&paul, &id).await;
}
