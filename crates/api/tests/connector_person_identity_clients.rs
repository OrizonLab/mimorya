//! Who authorizes a server is verified alike whatever its client
//! (ADR-175): a client without a secret, whose identity tokens are never
//! taken signed with a secret, while one with a secret may take them; and
//! the client the platform registered itself (ADR-173), the token issued to
//! it.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{CLIENT_ID, FakeAuthorizationServer, IdTokenQuirk};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::connector_oauth;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{
    PERSON_AUTHORIZED, PERSON_FAILED, person_authorize, person_kept,
};
use support::person_identity::{PERSON_OTHER_ACCOUNT, declare, refused_because, signed_in_as};
use support::sso::location;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_client_without_a_secret_never_takes_a_token_signed_with_one() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::MARC);
    let (_, id) = connector_oauth::declared(&paul, &authority, "public").await;
    // Declared again as a public client: its identifier alone.
    let public = json!({ "clientId": CLIENT_ID, "perPerson": true, "verifyIdentity": true });
    let declared = paul
        .put(&format!("/admin/connectors/{id}/oauth"), public)
        .await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    assert_eq!(
        listed(&paul, &id).await["mcp"]["oauth"]["clientAuth"],
        "none"
    );

    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_AUTHORIZED
    );
    // Signed with a secret it does not have: refused, nothing more kept.
    authority.set(|settings| settings.id_token_quirk = IdTokenQuirk::SignedWithSecret);
    let kept = person_kept(&api, &id, user::MARC).await;
    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_FAILED
    );
    assert_eq!(person_kept(&api, &id, user::MARC).await, kept);
    let reason = refused_because(&api, &id, user::MARC)
        .await
        .expect("refused");
    assert!(reason.contains("jeton d’identité"), "{reason}");

    // The same token, for a client with that secret: taken.
    let confidential = json!({ "perPerson": true, "verifyIdentity": true });
    let declared = declare(&paul, &id, confidential).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_AUTHORIZED
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_client_the_platform_registered_is_verified_alike() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    signed_in_as(&authority, user::MARC);
    let (_, id) = connector_oauth::declared(&paul, &authority, "enregistre").await;
    let registered = json!({ "register": true, "perPerson": true, "verifyIdentity": true });
    let declared = paul
        .put(&format!("/admin/connectors/{id}/oauth"), registered)
        .await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    let oauth = listed(&paul, &id).await["mcp"]["oauth"].clone();
    assert_eq!(oauth["registered"], true, "{oauth}");
    assert_eq!(oauth["verifiesIdentity"], true, "{oauth}");
    // Registered for the identity token's scopes.
    assert_eq!(authority.registered().len(), 1);

    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_AUTHORIZED
    );
    signed_in_as(&authority, user::PAUL);
    assert_eq!(
        location(&person_authorize(&marc, &authority, &id).await),
        PERSON_OTHER_ACCOUNT
    );
}
