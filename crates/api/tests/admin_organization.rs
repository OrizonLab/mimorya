//! Organisation administration: adding, renaming, moving and deleting units,
//! with the tree rules and the protection of units still in use.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{unit, user};
use serde_json::{Value, json};
use support::TestApi;

fn find_unit(organization: &Value, name: &str) -> Option<Value> {
    organization["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == name)
        .cloned()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn manages_the_organisation_tree() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let new_team = json!({ "parentId": unit::TECHNICAL, "name": "  Équipe Est ", "type": "team" });

    assert_eq!(
        marc.post("/admin/org-units", new_team.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        marc.get("/admin/organization").await.json()["canManage"],
        false
    );
    let created = paul.post("/admin/org-units", new_team).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_string();
    let url = format!("/admin/org-units/{id}");

    // The cached tree is refreshed at once.
    let tree = paul.get("/admin/organization").await.json();
    assert_eq!(tree["canManage"], true);
    let east = find_unit(&tree, "Équipe Est").expect("new unit listed");
    assert_eq!(east["parentId"], json!(unit::TECHNICAL));

    let renamed = paul
        .patch(
            &url,
            json!({ "name": "Équipe Est-Ouest", "leadUserId": user::LUCAS }),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    let tree = paul.get("/admin/organization").await.json();
    let east = find_unit(&tree, "Équipe Est-Ouest").unwrap();
    assert_eq!(east["leadUserId"], json!(user::LUCAS));
    assert_eq!(
        paul.patch(&url, json!({ "leadUserId": null })).await.status,
        StatusCode::OK
    );

    // The tree stays a tree.
    let cycle = paul
        .patch(
            &format!("/admin/org-units/{}", unit::TECHNICAL),
            json!({ "parentId": unit::TEAM_NORTH }),
        )
        .await;
    assert_eq!(cycle.status, StatusCode::CONFLICT);
    let unknown_type = paul.patch(&url, json!({ "type": "galaxie" })).await;
    assert_eq!(unknown_type.status, StatusCode::BAD_REQUEST);
    let moved = paul
        .patch(&url, json!({ "parentId": unit::DIRECTION }))
        .await;
    assert_eq!(moved.status, StatusCode::OK);

    // Units in use (members, rights, memory) and the root are kept.
    let in_use = paul
        .delete(&format!("/admin/org-units/{}", unit::TEAM_NORTH))
        .await;
    assert_eq!(in_use.status, StatusCode::CONFLICT);
    assert!(in_use.error_message().contains("membre"));
    let root = paul
        .delete(&format!("/admin/org-units/{}", unit::COMPANY))
        .await;
    assert_eq!(root.status, StatusCode::CONFLICT);
    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
    let tree = paul.get("/admin/organization").await.json();
    assert!(find_unit(&tree, "Équipe Est-Ouest").is_none());
}
