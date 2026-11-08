//! The company's accounts as its identity provider creates and reads them
//! through SCIM (ADR-183), with its token alone, without the header the
//! browser's changes carry: an account created as Entra ID sends it,
//! active, without a password, with the base role only, its address in
//! lowercase, read back by its identifier, by its address in any case or by
//! the provider's identifier, page after page; its creation audited by the
//! platform, naming the token; without a password, an administrator gives
//! it a temporary one. Refusals: `scim_refusals.rs`.

mod support;

use axum::http::StatusCode;
use axum::http::header::{CONTENT_TYPE, LOCATION};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::directory_entry;
use support::audit_exports::audit_log;
use support::scim::{SCIM_JSON, create, entra_user, get, made, total};
use uuid::Uuid;

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
    // It has no password: without the company's identity provider, an
    // administrator gives it a temporary one, with which the person signs in.
    let attempt = api
        .try_login("ines.garnier@neo-maintenance.example", "")
        .await;
    assert_ne!(attempt.status, StatusCode::OK);
    let given = paul
        .post(&format!("/admin/users/{id}/password"), json!({}))
        .await;
    assert_eq!(given.status, StatusCode::OK, "{}", given.text());
    let temporary = given.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    let signed_in = api
        .try_login("ines.garnier@neo-maintenance.example", &temporary)
        .await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());

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
