//! Who shares a unit's knowledge, and what a share may say (ADR-143): only
//! an administrator of the company's access policies (`policy:manage`,
//! company-wide), never one they would read through themself; between two
//! of its units, the one shared not containing the other, once, something
//! shared, until a date to come. An unknown unit is refused and traced, and
//! so is another company's; an unknown share is not found, nor another
//! company's; a unit is not deleted while shared.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{unit, user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::TestApi;
use support::knowledge_shares::{SHARES, share as shared, unshare};
use support::refusals::{
    entries_of_since, latest_entry, latest_entry_of, not_found_since, unknown_references_since,
};
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
    // With the units a share may name, each with its parent.
    let north = (listed["units"].as_array().unwrap().iter())
        .find(|u| u["id"] == json!(unit::TEAM_NORTH))
        .cloned();
    let expected =
        json!({ "id": unit::TEAM_NORTH, "parentId": unit::TECHNICAL, "name": "Équipe Nord" });
    assert_eq!(north, Some(expected), "{listed}");

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

/// Two units of the other company, the second's knowledge shared with the
/// first; that unit and the share.
async fn foreign_share(api: &TestApi) -> (Uuid, Uuid) {
    let (head, branch) = (Uuid::new_v4(), Uuid::new_v4());
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let units = [
        (head, None, "Siège voisin", "company"),
        (branch, Some(head), "Agence voisine", "team"),
    ];
    for (id, parent_id, name, kind) in units {
        sqlx::query(
            "insert into org_units (id, tenant_id, parent_id, name, type) \
             values ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(OTHER_TENANT_ID)
        .bind(parent_id)
        .bind(name)
        .bind(kind)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    let share: Uuid = sqlx::query_scalar(
        "insert into knowledge_shares (tenant_id, source_org_unit_id, target_org_unit_id, \
         shares_memory, shares_documents, max_sensitivity) \
         values ($1, $2, $3, true, false, 'internal') returning id",
    )
    .bind(OTHER_TENANT_ID)
    .bind(branch)
    .bind(head)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (head, share)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_units_and_shares_are_unknown_here() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (unit_elsewhere, share_elsewhere) = foreign_share(&api).await;
    let since = latest_entry(&api).await;
    let elsewhere = latest_entry_of(&api, OTHER_TENANT_ID).await;

    // Its unit: refused as unknown, traced.
    let body = share(unit::TEAM_NORTH, unit_elsewhere);
    let refused = paul.post(SHARES, body).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    let traced = unknown_references_since(&api, since).await;
    assert_eq!(traced.len(), 1, "{traced:?}");
    // Its share: neither listed nor removed here, traced.
    let listed = paul.get(SHARES).await.json();
    assert_eq!(listed["shares"], json!([]), "{listed}");
    let units = listed["units"].as_array().unwrap();
    assert!(
        !units.iter().any(|u| u["id"] == json!(unit_elsewhere)),
        "{listed}"
    );
    let path = format!("{SHARES}/{share_elsewhere}");
    assert_eq!(paul.delete(&path).await.status, StatusCode::NOT_FOUND);
    let traced = not_found_since(&api, since).await;
    assert_eq!(traced.len(), 1, "{traced:?}");
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let kept: bool =
        sqlx::query_scalar("select exists (select 1 from knowledge_shares where id = $1)")
            .bind(share_elsewhere)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(kept);
    // Nothing written in the other company's audit.
    assert_eq!(
        entries_of_since(&api, OTHER_TENANT_ID, elsewhere).await,
        "[]"
    );
}
