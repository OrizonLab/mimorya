//! The company's accounts as its identity provider creates and reads them
//! through SCIM (ADR-183), with its token alone, without the header the
//! browser's changes carry: an account created as Entra ID sends it,
//! active, without a password, with the base role only, its address in
//! lowercase, read back by its identifier, by its address in any case or by
//! the provider's identifier, page after page; its creation audited by the
//! platform, naming the token. Refusals are SCIM's, and create nothing.

mod support;

use axum::http::header::{CONTENT_TYPE, LOCATION, WWW_AUTHENTICATE};
use axum::http::{Method, StatusCode};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::directory_entry;
use support::audit_exports::audit_log;
use support::scim::{SCIM_JSON, create, entra_user, get, made, refusal, scim_as};
use uuid::Uuid;

/// How many accounts the provider sees.
async fn total(api: &TestApi, secret: &str) -> i64 {
    let listed = get(api, Some(secret), "/Users?count=0").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    listed.json()["totalResults"].as_i64().unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_identity_provider_creates_and_reads_accounts_with_its_token() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (token_id, secret) = made(&paul, "Microsoft Entra ID").await;
    let token = Some(secret.as_str());

    // What this server supports, said to the token's holder.
    let config = get(&api, token, "/ServiceProviderConfig").await;
    assert_eq!(config.status, StatusCode::OK, "{}", config.text());
    assert_eq!(config.headers[CONTENT_TYPE], SCIM_JSON);
    let config = config.json();
    assert_eq!(config["patch"]["supported"], false);
    assert_eq!(
        config["filter"],
        json!({ "supported": true, "maxResults": 200 })
    );
    assert_eq!(
        config["authenticationSchemes"][0]["type"],
        "oauthbearertoken"
    );

    // Created as Entra ID sends it: its address in lowercase, the attributes
    // kept, the others ignored.
    let before = total(&api, &secret).await;
    let created = create(
        &api,
        token,
        &entra_user("Ines.Garnier@Neo-Maintenance.example", "ines-1"),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    assert_eq!(created.headers[CONTENT_TYPE], SCIM_JSON);
    let account = created.json();
    let id: Uuid = account["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(account["userName"], "ines.garnier@neo-maintenance.example");
    assert_eq!(
        (
            &account["displayName"],
            &account["title"],
            &account["externalId"]
        ),
        (
            &json!("Inès Garnier"),
            &json!("Technicienne"),
            &json!("ines-1")
        )
    );
    assert_eq!(account["active"], true);
    assert_eq!(
        account["emails"],
        json!([{ "value": "ines.garnier@neo-maintenance.example", "type": "work", "primary": true }])
    );
    let location = account["meta"]["location"].as_str().unwrap();
    assert!(
        location.ends_with(&format!("/api/scim/v2/Users/{id}")),
        "{location}"
    );
    assert_eq!(created.headers[LOCATION], location);
    assert_eq!(total(&api, &secret).await, before + 1);

    // Read back by its identifier, its address in any case, the provider's
    // identifier.
    let read = get(&api, token, &format!("/Users/{id}")).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    assert_eq!(read.json(), account);
    for filter in [
        "userName%20eq%20%22INES.GARNIER%40neo-maintenance.example%22",
        "externalId%20eq%20%22ines-1%22",
    ] {
        let found = get(&api, token, &format!("/Users?filter={filter}")).await;
        assert_eq!(found.status, StatusCode::OK, "{}", found.text());
        let found = found.json();
        assert_eq!(
            found["schemas"],
            json!(["urn:ietf:params:scim:api:messages:2.0:ListResponse"])
        );
        assert_eq!(
            (&found["totalResults"], &found["itemsPerPage"]),
            (&json!(1), &json!(1))
        );
        assert_eq!(found["Resources"][0], account);
    }
    let nobody = get(&api, token, "/Users?filter=externalId%20eq%20%22INES-1%22").await;
    assert_eq!(nobody.json()["totalResults"], 0);

    // In the company: active, in no unit, the base role on its own data only.
    let entry = directory_entry(&paul, id).await;
    assert_eq!(entry["status"], "active");
    assert_eq!(entry["orgUnitId"], Value::Null);
    assert_eq!(entry["clearance"], "internal");
    let assignments = entry["assignments"].as_array().unwrap();
    assert_eq!(assignments.len(), 1, "{entry}");
    assert_eq!(assignments[0]["scope"]["type"], "self");
    // It has no password: no one signs in with one.
    let attempt = api
        .try_login("ines.garnier@neo-maintenance.example", "")
        .await;
    assert_ne!(attempt.status, StatusCode::OK);

    // Its creation audited by the platform, naming the token.
    let audited = audit_log(&paul)
        .await
        .into_iter()
        .find(|e| e["action"] == "user.created" && e["resourceId"] == json!(id))
        .expect("creation audited");
    assert_eq!(audited["actorType"], "system");
    assert_eq!(
        audited["metadata"],
        json!({ "via": "scim", "scimToken": token_id, "clearance": "internal", "active": true })
    );

    // Created inactive, it is disabled.
    let mut inactive = entra_user("absent.depuis@neo-maintenance.example", "absent-1");
    inactive["active"] = json!("False");
    inactive["displayName"] = json!(null);
    let created = create(&api, token, &inactive).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let inactive = created.json();
    assert_eq!(inactive["active"], false);
    assert_eq!(inactive["displayName"], "Inès Garnier");
    let id: Uuid = inactive["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(directory_entry(&paul, id).await["status"], "disabled");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_provider_reads_the_accounts_page_after_page() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, secret) = made(&paul, "Okta").await;
    let token = Some(secret.as_str());
    let all = total(&api, &secret).await;
    assert!(all > 3, "{all}");

    let first = get(&api, token, "/Users?startIndex=1&count=2").await.json();
    assert_eq!(
        (
            &first["totalResults"],
            &first["startIndex"],
            &first["itemsPerPage"]
        ),
        (&json!(all), &json!(1), &json!(2))
    );
    let second = get(&api, token, "/Users?startIndex=3&count=2").await.json();
    assert_eq!(second["startIndex"], 3);
    let ids = |page: &Value| -> Vec<Value> {
        page["Resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].clone())
            .collect()
    };
    let (first, second) = (ids(&first), ids(&second));
    assert_eq!(second.len(), 2);
    assert!(first.iter().all(|id| !second.contains(id)));
    // Every account, once, in the order they were made.
    let whole = get(&api, token, "/Users").await.json();
    let whole = ids(&whole);
    assert_eq!(whole.len() as i64, all);
    assert_eq!(&whole[..4], [first, second].concat().as_slice());
    // Past the end, or a page of none: no account, the total all the same;
    // a start below 1 reads as 1, a count above the most as the most.
    let past = get(&api, token, &format!("/Users?startIndex={}", all + 1))
        .await
        .json();
    assert_eq!(
        (&past["totalResults"], &past["itemsPerPage"]),
        (&json!(all), &json!(0))
    );
    let none = get(&api, token, "/Users?count=0").await.json();
    assert_eq!(none["Resources"], json!([]));
    let low = get(&api, token, "/Users?startIndex=-4&count=1")
        .await
        .json();
    assert_eq!(
        (&low["startIndex"], &low["Resources"][0]["id"]),
        (&json!(1), &whole[0])
    );
    let most = get(&api, token, "/Users?count=100000").await.json();
    assert_eq!(most["itemsPerPage"].as_i64().unwrap(), all.min(200));
    // A filtered page past its only account holds none.
    let email = "paul.renaud%40neo-maintenance.example";
    let filtered = get(
        &api,
        token,
        &format!("/Users?filter=userName%20eq%20%22{email}%22&startIndex=2"),
    )
    .await;
    let filtered = filtered.json();
    assert_eq!(
        (&filtered["totalResults"], &filtered["Resources"]),
        (&json!(1), &json!([]))
    );
}

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
