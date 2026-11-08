//! How SCIM refuses (ADR-183): as SCIM writes it, creating nothing. Without
//! a token, any request under the prefix is refused as such, whatever else
//! is wrong with it, a method or a path it does not serve included; with
//! one, a refused request still notes its use, and a method or a path not
//! served is refused as SCIM refuses (405, 404).

mod support;

use axum::http::header::WWW_AUTHENTICATE;
use axum::http::{Method, StatusCode};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::scim::{
    SCIM_JSON, create, entra_user, get, last_used, made, refusal, removed, scim_as, total,
};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refusals_are_scim_s_and_create_nothing() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, secret) = made(&paul, "Microsoft Entra ID").await;
    let token = Some(secret.as_str());
    let before = total(&api, &secret).await;

    // Without a token, or with an unknown one: the same refusal.
    let anonymous = get(&api, None, "/Users").await;
    assert_eq!(refusal(&anonymous), (StatusCode::UNAUTHORIZED, Value::Null));
    assert!(anonymous.headers.contains_key(WWW_AUTHENTICATE));
    let unknown = get(&api, Some("msc_inconnu"), "/Users").await;
    assert_eq!(unknown.text(), anonymous.text());
    let created = create(
        &api,
        Some("msc_inconnu"),
        &entra_user("x@neo-maintenance.example", "x"),
    )
    .await;
    assert_eq!(created.status, StatusCode::UNAUTHORIZED);

    // A filter read loosely would match the wrong account.
    for filter in [
        "userName%20co%20%22paul%22",
        "userName%20eq%20%22a%40b.example%22%20or%20userName%20eq%20%22c%40d.example%22",
        "displayName%20eq%20%22Paul%22",
    ] {
        let refused = get(&api, token, &format!("/Users?filter={filter}")).await;
        assert_eq!(
            refusal(&refused),
            (StatusCode::BAD_REQUEST, json!("invalidFilter"))
        );
    }
    let refused = get(&api, token, "/Users?startIndex=premier").await;
    assert_eq!(
        refusal(&refused),
        (StatusCode::BAD_REQUEST, json!("invalidValue"))
    );
    for path in [
        format!("/Users/{}", Uuid::new_v4()),
        "/Users/inconnu".to_string(),
    ] {
        let missing = get(&api, token, &path).await;
        assert_eq!(refusal(&missing), (StatusCode::NOT_FOUND, Value::Null));
    }

    // An address already used, here or elsewhere; an identifier the provider
    // already gave another account.
    let taken = create(
        &api,
        token,
        &entra_user("Paul.Renaud@neo-maintenance.example", "p"),
    )
    .await;
    assert_eq!(refusal(&taken), (StatusCode::CONFLICT, json!("uniqueness")));
    let first = create(
        &api,
        token,
        &entra_user("une@neo-maintenance.example", "meme"),
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.text());
    let same = create(
        &api,
        token,
        &entra_user("autre@neo-maintenance.example", "meme"),
    )
    .await;
    assert_eq!(refusal(&same), (StatusCode::CONFLICT, json!("uniqueness")));

    // What the account could not hold, or a body not read.
    for user in [
        json!({ "displayName": "Sans adresse" }),
        json!({ "userName": "pas-une-adresse" }),
        json!({ "userName": "nom@neo-maintenance.example", "displayName": "N" }),
        json!({ "userName": "actif@neo-maintenance.example", "active": "oui" }),
        json!({ "userName": "nul@neo-maintenance.example", "title": "\u{0}" }),
    ] {
        let refused = create(&api, token, &user).await;
        assert_eq!(
            refusal(&refused),
            (StatusCode::BAD_REQUEST, json!("invalidValue")),
            "{user}"
        );
    }
    let unread = scim_as(
        &api,
        token,
        Method::POST,
        "/Users",
        Some((SCIM_JSON, "{".into())),
    )
    .await;
    assert_eq!(
        refusal(&unread),
        (StatusCode::BAD_REQUEST, json!("invalidSyntax"))
    );
    let body = entra_user("texte@neo-maintenance.example", "texte").to_string();
    let as_text = scim_as(
        &api,
        token,
        Method::POST,
        "/Users",
        Some(("text/plain", body)),
    )
    .await;
    assert_eq!(
        refusal(&as_text),
        (StatusCode::UNSUPPORTED_MEDIA_TYPE, Value::Null)
    );
    // As plain JSON, it is read all the same.
    let body = entra_user("json@neo-maintenance.example", "json").to_string();
    let as_json = scim_as(
        &api,
        token,
        Method::POST,
        "/Users",
        Some(("application/json; charset=utf-8", body)),
    )
    .await;
    assert_eq!(as_json.status, StatusCode::CREATED, "{}", as_json.text());

    // Only the two accounts accepted were created.
    assert_eq!(total(&api, &secret).await, before + 2);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_token_any_request_is_refused_as_such() {
    let api = TestApi::start().await;
    let unknown = get(&api, None, "/Users").await;
    assert_eq!(refusal(&unknown), (StatusCode::UNAUTHORIZED, Value::Null));
    let body = entra_user("sans.jeton@neo-maintenance.example", "s").to_string();
    for (method, path, body) in [
        (Method::GET, "/Users?startIndex=premier", None),
        (
            Method::GET,
            "/Users?filter=displayName%20eq%20%22x%22",
            None,
        ),
        (Method::GET, "/Users/inconnu", None),
        (Method::GET, "/ServiceProviderConfig", None),
        (Method::POST, "/Users", Some((SCIM_JSON, "{".to_string()))),
        (Method::POST, "/Users", Some(("text/plain", body.clone()))),
        (Method::POST, "/Users", Some((SCIM_JSON, body.clone()))),
        // Whatever the path or the method, served or not.
        (
            Method::PUT,
            "/Users/inconnu",
            Some((SCIM_JSON, body.clone())),
        ),
        (
            Method::PATCH,
            "/Users/inconnu",
            Some((SCIM_JSON, "{".to_string())),
        ),
        (Method::DELETE, "/Users/inconnu", None),
        (Method::DELETE, "/Users", None),
        (Method::PATCH, "/ServiceProviderConfig", None),
        (Method::GET, "/Users/%FF", None),
        (Method::GET, "/Groups", None),
        (Method::POST, "/Bulk", Some((SCIM_JSON, "{}".to_string()))),
    ] {
        for token in [None, Some("msc_inconnu")] {
            let refused = scim_as(&api, token, method.clone(), path, body.clone()).await;
            assert_eq!(refused.status, StatusCode::UNAUTHORIZED, "{method} {path}");
            assert_eq!(refused.text(), unknown.text(), "{method} {path}");
        }
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_refused_request_still_notes_the_token_s_use() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let taken = entra_user("paul.renaud@neo-maintenance.example", "pris").to_string();
    let unnamed = json!({ "userName": "x" }).to_string();
    for (method, path, body) in [
        (
            Method::GET,
            "/Users?filter=displayName%20eq%20%22x%22".to_string(),
            None,
        ),
        (Method::GET, "/Users?count=beaucoup".to_string(), None),
        (Method::GET, format!("/Users/{}", Uuid::new_v4()), None),
        (
            Method::POST,
            "/Users".to_string(),
            Some((SCIM_JSON, "{".to_string())),
        ),
        (
            Method::POST,
            "/Users".to_string(),
            Some(("text/plain", taken.clone())),
        ),
        (
            Method::POST,
            "/Users".to_string(),
            Some((SCIM_JSON, taken.clone())),
        ),
        (
            Method::POST,
            "/Users".to_string(),
            Some((SCIM_JSON, unnamed.clone())),
        ),
        (
            Method::PATCH,
            format!("/Users/{}", Uuid::new_v4()),
            Some((SCIM_JSON, json!({ "Operations": [] }).to_string())),
        ),
        (
            Method::PUT,
            format!("/Users/{}", user::PAUL),
            Some((
                SCIM_JSON,
                json!({ "userName": "autre@neo-maintenance.example" }).to_string(),
            )),
        ),
        (Method::DELETE, format!("/Users/{}", Uuid::new_v4()), None),
        // A method or a path it does not serve.
        (Method::DELETE, "/Users".to_string(), None),
        (Method::GET, "/Users/%FF".to_string(), None),
        (Method::GET, "/Groups".to_string(), None),
    ] {
        // Its only request, refused.
        let (id, secret) = made(&paul, "Fournisseur refusé").await;
        assert_eq!(last_used(&paul, &id).await, Value::Null);
        let refused = scim_as(&api, Some(&secret), method.clone(), &path, body).await;
        assert!(
            refused.status.is_client_error(),
            "{method} {path}: {}",
            refused.text()
        );
        assert_ne!(refused.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert!(last_used(&paul, &id).await.is_string(), "{method} {path}");
        removed(&paul, &id).await;
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_the_prefix_does_not_serve_is_refused_as_scim_refuses() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (id, secret) = made(&paul, "Fournisseur égaré").await;
    let token = Some(secret.as_str());
    for (method, path, status) in [
        (Method::DELETE, "/Users", StatusCode::METHOD_NOT_ALLOWED),
        (
            Method::PUT,
            "/ServiceProviderConfig",
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (Method::GET, "/Groups", StatusCode::NOT_FOUND),
        (Method::GET, "/Users/%FF", StatusCode::NOT_FOUND),
        (Method::GET, "/Users/inconnu/plus", StatusCode::NOT_FOUND),
    ] {
        let refused = scim_as(&api, token, method.clone(), path, None).await;
        assert_eq!(refusal(&refused), (status, Value::Null), "{method} {path}");
    }
    assert!(last_used(&paul, &id).await.is_string());
    removed(&paul, &id).await;
}
