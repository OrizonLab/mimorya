//! Who shares a unit's knowledge, and what a share may say (ADR-143): only
//! an administrator of the company's access policies (`policy:manage`,
//! company-wide), never one they would read through themself; between two
//! of its units, the one shared not containing the other, once, something
//! shared, until a date to come. An unknown unit is refused and traced; an
//! unknown share is not found; a unit is not deleted while shared.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{unit, user};
use serde_json::{Value, json};
use support::TestApi;
use support::knowledge_shares::{SHARES, share as shared, unshare};
use support::refusals::{latest_entry, unknown_references_since};
use uuid::Uuid;

fn share(source: Uuid, target: Uuid) -> Value {
    json!({
        "sourceOrgUnitId": source,
        "targetOrgUnitId": target,
        "memory": true,
        "documents": true,
        "maxSensitivity": "internal",
    })
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_policies_administrator_shares_and_within_the_rules() {
    let api = TestApi::start().await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);

    // Not an administrator of the access policies.
    let north_to_south = share(unit::TEAM_NORTH, unit::TEAM_SOUTH);
    assert_eq!(marc.get(SHARES).await.status, StatusCode::FORBIDDEN);
    let refused = marc.post(SHARES, north_to_south.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());

    // What a share may not say.
    let mut nothing = north_to_south.clone();
    nothing["memory"] = json!(false);
    nothing["documents"] = json!(false);
    let mut past = north_to_south.clone();
    past["expiresAt"] = json!("2000-01-01T00:00:00Z");
    let mut unknown_field = north_to_south.clone();
    unknown_field["write"] = json!(true);
    for body in [
        nothing,
        past,
        share(unit::TEAM_NORTH, unit::TEAM_NORTH),
        // Its members read their service's knowledge already.
        share(unit::TECHNICAL, unit::TEAM_NORTH),
        unknown_field,
    ] {
        let refused = paul.post(SHARES, body.clone()).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{body}: {}",
            refused.text()
        );
    }

    // Never one he would read through: it takes another administrator.
    let mut everyone = share(unit::TEAM_NORTH, unit::COMPANY);
    everyone["includeDescendants"] = json!(true);
    for body in [share(unit::TEAM_NORTH, unit::ADMIN), everyone] {
        let refused = paul.post(SHARES, body.clone()).await;
        assert_eq!(
            refused.status,
            StatusCode::FORBIDDEN,
            "{body}: {}",
            refused.text()
        );
    }
    // The company's own members only: not him.
    let company = paul
        .post(SHARES, share(unit::TEAM_NORTH, unit::COMPANY))
        .await;
    assert_eq!(company.status, StatusCode::CREATED, "{}", company.text());
    unshare(&paul, company.json()["id"].as_str().unwrap()).await;

    // An unknown unit: refused, traced.
    let since = latest_entry(&api).await;
    let elsewhere = Uuid::new_v4();
    let refused = paul.post(SHARES, share(unit::TEAM_NORTH, elsewhere)).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    let traced = unknown_references_since(&api, since).await;
    assert_eq!(traced.len(), 1, "{traced:?}");

    // Once between two units.
    let created = paul.post(SHARES, north_to_south.clone()).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let again = paul.post(SHARES, north_to_south).await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.text());
    let listed = paul.get(SHARES).await.json();
    assert_eq!(listed["shares"].as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed["shares"][0]["maxSensitivity"], "internal");

    // Removed by him only; an unknown share is not found.
    let id = created.json()["id"].as_str().unwrap().to_owned();
    let path = format!("{SHARES}/{id}");
    assert_eq!(marc.delete(&path).await.status, StatusCode::FORBIDDEN);
    assert_eq!(paul.delete(&path).await.status, StatusCode::OK);
    assert_eq!(paul.delete(&path).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_unit_is_deleted_only_once_its_shares_are_removed() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let unit_body = json!({ "parentId": unit::COMPANY, "name": "Cellule qualité", "type": "team" });
    let created = paul.post("/admin/org-units", unit_body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let quality: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    let path = format!("/admin/org-units/{quality}");

    for units in [(unit::TEAM_NORTH, quality), (quality, unit::TEAM_NORTH)] {
        let made = shared(&paul, share(units.0, units.1)).await;
        let refused = paul.delete(&path).await;
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
        assert!(refused.text().contains("partage"), "{}", refused.text());
        unshare(&paul, &made).await;
    }
    let removed = paul.delete(&path).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}
