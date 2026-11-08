//! The company's accounts as its identity provider changes and removes them
//! through SCIM (ADR-183): a patch as Entra ID sends it and a replacement
//! as Okta sends it change the name, the job title, the provider's
//! identifier and whether the account is active; never its address. A
//! disabled account loses its sessions at once; a removed one is disabled
//! and out of SCIM's sight until an administrator enables it again. Each
//! change audited by the platform, naming the token.

mod support;

use axum::http::{Method, StatusCode};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::directory_entry;
use support::audit_exports::audit_log;
use support::scim::{create, entra_user, get, made, patch_of, refusal, send};
use uuid::Uuid;

const ADDRESS: &str = "lea.change@neo-maintenance.example";

/// The identifier of the account the provider holding `secret` creates at
/// `address`.
async fn created(api: &TestApi, secret: &str, address: &str, external_id: &str) -> Uuid {
    let made = create(api, Some(secret), &entra_user(address, external_id)).await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.text());
    made.json()["id"].as_str().unwrap().parse().unwrap()
}

/// The last audit entry `action` of the account `id`.
async fn last_entry(paul: &support::Client<'_>, action: &str, id: Uuid) -> Value {
    audit_log(paul)
        .await
        .into_iter()
        .find(|e| e["action"] == action && e["resourceId"] == json!(id))
        .unwrap_or_else(|| panic!("{action} audited"))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_provider_changes_an_account_never_its_address() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (token_id, secret) = made(&paul, "Microsoft Entra ID").await;
    let token = Some(secret.as_str());
    let id = created(&api, &secret, ADDRESS, "lea-1").await;
    let path = format!("/Users/{id}");

    // A patch as Entra ID sends it: the attributes kept changed, the others
    // read by no one.
    let operations = patch_of(json!([
        { "op": "Replace", "path": "displayName", "value": "Léa Change" },
        { "op": "Replace", "path": "title", "value": "Cheffe d’équipe" },
        { "op": "Replace", "path": "externalId", "value": "lea-2" },
        { "op": "Add", "path": "phoneNumbers[type eq \"work\"].value", "value": "0102030405" },
        { "op": "Replace", "path": "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User:department", "value": "Lyon" }
    ]));
    let patched = send(&api, token, Method::PATCH, &path, &operations).await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text());
    let account = patched.json();
    assert_eq!(
        (
            &account["displayName"],
            &account["title"],
            &account["externalId"]
        ),
        (
            &json!("Léa Change"),
            &json!("Cheffe d’équipe"),
            &json!("lea-2")
        )
    );
    assert!(!patched.text().contains("0102030405"));
    let entry = directory_entry(&paul, id).await;
    assert_eq!(
        (&entry["displayName"], &entry["jobTitle"]),
        (&json!("Léa Change"), &json!("Cheffe d’équipe"))
    );
    let audited = last_entry(&paul, "user.updated", id).await;
    assert_eq!(audited["actorType"], "system");
    assert_eq!(
        audited["metadata"],
        json!({ "via": "scim", "scimToken": token_id, "changed": ["displayName", "title", "externalId"] })
    );
    let by_new = get(&api, token, "/Users?filter=externalId%20eq%20%22lea-2%22").await;
    assert_eq!(by_new.json()["totalResults"], 1);

    // A replacement as Okta sends it: what it sends, its title emptied.
    let replaced = json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
        "userName": ADDRESS.to_uppercase(),
        "name": { "givenName": "Léa", "familyName": "Remplacée" },
        "title": "",
        "active": true
    });
    let put = send(&api, token, Method::PUT, &path, &replaced).await;
    assert_eq!(put.status, StatusCode::OK, "{}", put.text());
    let account = put.json();
    assert_eq!(account["displayName"], "Léa Remplacée");
    assert!(account.get("title").is_none(), "{account}");
    assert_eq!(account["externalId"], "lea-2");
    // Its identifier taken away by a patch.
    let removal = patch_of(json!([{ "op": "remove", "path": "externalId" }]));
    let removed = send(&api, token, Method::PATCH, &path, &removal).await;
    assert!(
        removed.json().get("externalId").is_none(),
        "{}",
        removed.text()
    );

    // Its address is the account's for good: another one is refused, and
    // nothing changes.
    let other = "autre.adresse@neo-maintenance.example";
    let renamed = patch_of(json!([
        { "op": "replace", "path": "displayName", "value": "Ne change pas" },
        { "op": "replace", "path": "userName", "value": other }
    ]));
    let refused = send(&api, token, Method::PATCH, &path, &renamed).await;
    assert_eq!(
        refusal(&refused),
        (StatusCode::BAD_REQUEST, json!("mutability"))
    );
    let refused = send(
        &api,
        token,
        Method::PUT,
        &path,
        &json!({ "userName": other }),
    )
    .await;
    assert_eq!(
        refusal(&refused),
        (StatusCode::BAD_REQUEST, json!("mutability"))
    );
    assert_eq!(
        directory_entry(&paul, id).await["displayName"],
        "Léa Remplacée"
    );

    // An identifier another account holds; what cannot be read or go.
    created(
        &api,
        &secret,
        "voisin.change@neo-maintenance.example",
        "voisin",
    )
    .await;
    let taken = patch_of(json!([{ "op": "add", "path": "externalId", "value": "voisin" }]));
    let refused = send(&api, token, Method::PATCH, &path, &taken).await;
    assert_eq!(
        refusal(&refused),
        (StatusCode::CONFLICT, json!("uniqueness"))
    );
    for (operations, kind) in [
        (json!({ "Operations": [] }), "invalidSyntax"),
        (json!({ "operations": "remove" }), "invalidSyntax"),
        (
            patch_of(json!([{ "op": "move", "path": "title" }])),
            "invalidSyntax",
        ),
        (
            patch_of(json!([{ "op": "replace", "path": "displayName", "value": "L" }])),
            "invalidValue",
        ),
        (
            patch_of(json!([{ "op": "replace", "path": "active", "value": "peut-être" }])),
            "invalidValue",
        ),
        (
            patch_of(json!([{ "op": "remove", "path": "active" }])),
            "mutability",
        ),
    ] {
        let refused = send(&api, token, Method::PATCH, &path, &operations).await;
        assert_eq!(
            refusal(&refused),
            (StatusCode::BAD_REQUEST, json!(kind)),
            "{operations}"
        );
    }
    assert_eq!(
        directory_entry(&paul, id).await["displayName"],
        "Léa Remplacée"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_account_loses_its_sessions_at_once() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (token_id, secret) = made(&paul, "Microsoft Entra ID").await;
    let token = Some(secret.as_str());
    let address = "desactive.scim@neo-maintenance.example";
    let id = created(&api, &secret, address, "desactive").await;
    let given = paul
        .post(&format!("/admin/users/{id}/password"), json!({}))
        .await;
    let temporary = given.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    let session = api.sign_in_with(address, &temporary).await;
    assert_eq!(session.get("/auth/me").await.status, StatusCode::OK);

    // Disabled as Entra ID does it: at once, its session ends.
    let path = format!("/Users/{id}");
    let off = patch_of(json!([{ "op": "Replace", "path": "active", "value": "False" }]));
    let disabled = send(&api, token, Method::PATCH, &path, &off).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(disabled.json()["active"], false);
    assert_eq!(directory_entry(&paul, id).await["status"], "disabled");
    assert_eq!(
        session.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
    let audited = last_entry(&paul, "user.disabled", id).await;
    assert_eq!(
        (&audited["actorType"], &audited["metadata"]),
        (
            &json!("system"),
            &json!({ "via": "scim", "scimToken": token_id, "changed": ["active"] })
        )
    );
    assert_ne!(
        api.try_login(address, &temporary).await.status,
        StatusCode::OK
    );

    // Enabled again as Okta does it.
    let on = json!({ "userName": address, "active": true });
    let enabled = send(&api, token, Method::PUT, &path, &on).await;
    assert_eq!(enabled.json()["active"], true, "{}", enabled.text());
    assert_eq!(directory_entry(&paul, id).await["status"], "active");
    last_entry(&paul, "user.enabled", id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_removed_account_is_out_of_sight_until_enabled_again() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (token_id, secret) = made(&paul, "Okta").await;
    let token = Some(secret.as_str());
    let address = "retire.scim@neo-maintenance.example";
    let id = created(&api, &secret, address, "retire").await;
    let path = format!("/Users/{id}");

    let removed = send(&api, token, Method::DELETE, &path, &Value::Null).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.text());
    assert!(removed.body.is_empty());
    // Disabled, and no longer there for SCIM, by any way.
    assert_eq!(directory_entry(&paul, id).await["status"], "disabled");
    let audited = last_entry(&paul, "user.disabled", id).await;
    assert_eq!(
        audited["metadata"],
        json!({ "via": "scim", "scimToken": token_id, "removed": true, "wasDisabled": false })
    );
    for path in [
        path.clone(),
        "/Users?filter=userName%20eq%20%22retire.scim%40neo-maintenance.example%22".into(),
        "/Users?filter=externalId%20eq%20%22retire%22".into(),
    ] {
        let read = get(&api, token, &path).await;
        let gone = read.status == StatusCode::NOT_FOUND || read.json()["totalResults"] == 0;
        assert!(gone, "{path}: {}", read.text());
    }
    let listed = get(&api, token, "/Users").await.text();
    assert!(!listed.contains(address));
    for (method, body) in [
        (Method::DELETE, Value::Null),
        (Method::PUT, json!({ "userName": address, "active": true })),
        (
            Method::PATCH,
            patch_of(json!([{ "op": "replace", "path": "active", "value": true }])),
        ),
    ] {
        let refused = send(&api, token, method.clone(), &path, &body).await;
        assert_eq!(
            refusal(&refused),
            (StatusCode::NOT_FOUND, Value::Null),
            "{method}"
        );
    }
    // Its address is still its: the provider cannot make another account
    // with it.
    let again = create(&api, token, &entra_user(address, "retire-2")).await;
    assert_eq!(refusal(&again), (StatusCode::CONFLICT, json!("uniqueness")));

    // Enabled again by an administrator, it is there again.
    let enabled = paul
        .patch(&format!("/admin/users/{id}"), json!({ "status": "active" }))
        .await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    let back = get(&api, token, &path).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.text());
    assert_eq!(
        (&back.json()["active"], &back.json()["externalId"]),
        (&json!(true), &json!("retire"))
    );
}
