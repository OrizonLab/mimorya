//! A server each person authorizes may verify who authorizes it (ADR-175):
//! its authorization server, an OpenID provider, proves by an identity
//! token, verified against its published keys, for the declared client and
//! the attempt's nonce, that the account authorized is the person's own.
//! Another person's account, an address the server does not vouch for, a
//! missing or spoiled token: refused, nothing kept, the tokens granted
//! revoked, the reason audited. Never asked of a server without it, nor of
//! the company's authorization, nor declared for a server that is no
//! OpenID provider.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeAuthorizationServer, IdTokenQuirk, SignedIn};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{
    PERSON_AUTHORIZED, PERSON_FAILED, declared_per_person, last_metadata, person_authorize,
    person_kept, person_start,
};
use support::person_identity::{declare, declared_verifying, refused_because, signed_in_as};
use support::person_renewals::{asked_at, revoked_until};
use support::sso::location;
use support::{connector_oauth, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_proves_the_account_they_authorize_with_is_theirs() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::MARC);
    let (resource, id) = declared_verifying(&paul, &authority, "identite").await;
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["verifiesIdentity"], true, "{oauth}");
    for scope in ["openid", "email", "profile"] {
        assert!(
            oauth["scopes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s == scope),
            "{scope}: {oauth}"
        );
    }

    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_AUTHORIZED
    );
    let token = person_kept(&api, &id, user::MARC).await.expect("kept");
    assert!(authority.accepts(&token, &resource));
    // Asked with a nonce, and for the identity token's scopes.
    let asked = asked_at(&authority, "authorize");
    let params = &asked.last().expect("asked").params;
    assert!(params.get("nonce").is_some_and(|nonce| nonce.len() >= 16));
    assert!(params["scope"].split(' ').any(|scope| scope == "openid"));
    assert!(refused_because(&api, &id, user::MARC).await.is_none());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_person_s_account_is_refused_and_its_tokens_revoked() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    // Marc signs in at the server as Paul.
    signed_in_as(&authority, user::PAUL);
    let (_, id) = declared_verifying(&paul, &authority, "autrecompte").await;

    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_FAILED
    );
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    let reason = refused_because(&api, &id, user::MARC)
        .await
        .expect("refused");
    assert!(reason.contains("autre adresse"), "{reason}");
    // Granted, kept nowhere: revoked, access and refresh.
    assert_eq!(revoked_until(&authority, 2).await.len(), 2);
    // Paul, signed in as himself, is not refused.
    signed_in_as(&authority, user::PAUL);
    assert_eq!(
        location(&person_authorize(&paul, &authority, &id).await),
        PERSON_AUTHORIZED
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_address_the_server_does_not_vouch_for_proves_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let unverified = SignedIn {
        email_verified: false,
        ..SignedIn::verified(&email(user::MARC))
    };
    authority.set(|settings| settings.identity = Some(unverified));
    let (_, id) = declared_verifying(&paul, &authority, "nongarantie").await;

    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_FAILED
    );
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    let reason = refused_because(&api, &id, user::MARC)
        .await
        .expect("refused");
    assert!(reason.contains("ne garantit pas"), "{reason}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_missing_or_spoiled_identity_token_is_refused() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::MARC);
    let (_, id) = declared_verifying(&paul, &authority, "jetonabime").await;

    let spoiled = [
        (IdTokenQuirk::Missing, "pas donné de jeton d’identité"),
        (IdTokenQuirk::OtherNonce, "jeton d’identité"),
        (IdTokenQuirk::OtherAudience, "jeton d’identité"),
        (IdTokenQuirk::Forged, "jeton d’identité"),
        (IdTokenQuirk::Expired, "jeton d’identité"),
    ];
    for (quirk, said) in spoiled {
        authority.set(|settings| settings.id_token_quirk = quirk);
        assert_eq!(
            location(&person_authorize(&marc, &authority, &id).await),
            PERSON_FAILED,
            "{quirk:?}"
        );
        assert!(
            person_kept(&api, &id, user::MARC).await.is_none(),
            "{quirk:?}"
        );
        let reason = refused_because(&api, &id, user::MARC)
            .await
            .expect("refused");
        assert!(reason.contains(said), "{quirk:?}: {reason}");
    }
    // Each grant revoked: two tokens each.
    assert_eq!(revoked_until(&authority, 10).await.len(), 10);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_the_verification_no_identity_is_asked() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    // Signed in as another: nobody checks.
    signed_in_as(&authority, user::PAUL);
    let (_, id) = declared_per_person(&paul, &authority, "sansverif").await;
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["verifiesIdentity"], false, "{oauth}");

    let (url, _) = person_start(&marc, &id).await;
    assert!(!url.contains("nonce="), "{url}");
    assert!(!url.contains("openid"), "{url}");
    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_AUTHORIZED
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_s_authorization_never_verifies_identities() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::PAUL);
    let (_, id) = connector_oauth::declared(&paul, &authority, "entreprise").await;

    // Asked for the company's, said or kept: refused, nothing changed.
    for body in [
        json!({ "verifyIdentity": true }),
        json!({ "verifyIdentity": true, "perPerson": false }),
    ] {
        let refused = declare(&paul, &id, body.clone()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            refused.text().contains("pour elle-même"),
            "{}",
            refused.text()
        );
    }
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["verifiesIdentity"], false);
    assert_eq!(oauth["perPerson"], false);

    // Verifying for each person, then the company's: no longer verified,
    // nor once each person's again.
    let verifying = declare(
        &paul,
        &id,
        json!({ "perPerson": true, "verifyIdentity": true }),
    )
    .await;
    assert_eq!(verifying.status, StatusCode::OK, "{}", verifying.text());
    for per_person in [false, true] {
        let changed = paul
            .put(
                &format!("/admin/connectors/{id}/oauth/holder"),
                json!({ "perPerson": per_person }),
            )
            .await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
        let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
        assert_eq!(oauth["verifiesIdentity"], false, "{per_person}: {oauth}");
        // Audited as removed when it was there, the first time only.
        let audited = last_metadata(&api, &id, "connector.oauth_holder_changed").await;
        assert_eq!(audited["verificationRemoved"], !per_person, "{audited}");
    }
    // Written again for each person, without a word of it: kept.
    let again = declare(&paul, &id, json!({ "verifyIdentity": true })).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    let kept = declare(&paul, &id, json!({})).await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.text());
    assert_eq!(
        listed(&paul, &id).await["mcp"]["oauth"]["verifiesIdentity"],
        true
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_server_that_is_no_openid_provider_is_not_declared_so() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    // Nobody signed in: no OpenID configuration.
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "pasoidc").await;

    let refused = declare(&paul, &id, json!({ "verifyIdentity": true })).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(
        refused.text().contains("OpenID Connect"),
        "{}",
        refused.text()
    );
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["verifiesIdentity"], false);
    assert_eq!(oauth["perPerson"], true);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_attempt_started_before_the_verification_keeps_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::PAUL);
    let (_, id) = declared_per_person(&paul, &authority, "avantverif").await;
    let (url, cookie) = person_start(&marc, &id).await;
    // Meanwhile, the declaration starts verifying identities.
    let verifying = declare(&paul, &id, json!({ "verifyIdentity": true })).await;
    assert_eq!(verifying.status, StatusCode::OK, "{}", verifying.text());

    let back = connector_oauth::api_path(&authority.sent_back(&url).await);
    let returned = connector_oauth::come_back(&marc, &back, Some(&cookie)).await;
    assert_eq!(location(&returned), PERSON_FAILED);
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
}
