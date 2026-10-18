//! Organisation administration: adding, renaming, moving and deleting units,
//! with the tree rules and the protection of units still in use.

mod support;

use axum::http::StatusCode;
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{role, tenant, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_db::repositories::memory::{NewMemory, create_memory, delete_memory};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::{Value, json};
use support::accounts::create_account;
use support::{Client, TestApi};
use uuid::Uuid;

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

    // Units in use and the root are kept.
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

async fn create_unit(paul: &Client<'_>, parent: Uuid, name: &str) -> String {
    let created = paul
        .post(
            "/admin/org-units",
            json!({ "parentId": parent, "name": name, "type": "team" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_string()
}

/// Deleting the unit is refused, naming what still uses it.
async fn assert_in_use(paul: &Client<'_>, url: &str, what: &str) {
    let refused = paul.delete(url).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{what}");
    assert!(
        refused.error_message().contains(what),
        "{}",
        refused.error_message()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn keeps_each_kind_of_use_from_being_deleted() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = create_unit(&paul, unit::TECHNICAL, "Équipe Ouest").await;
    let unit_id: Uuid = id.parse().unwrap();
    let url = format!("/admin/org-units/{id}");

    let child = create_unit(&paul, unit_id, "Cellule Ouest").await;
    assert_in_use(&paul, &url, "sous-unité").await;
    let removed = paul.delete(&format!("/admin/org-units/{child}")).await;
    assert_eq!(removed.status, StatusCode::OK);

    let account = create_account(&paul, "membre.ouest").await;
    let account_url = format!("/admin/users/{}", account.id);
    paul.patch(&account_url, json!({ "orgUnitId": unit_id }))
        .await;
    assert_in_use(&paul, &url, "membre").await;
    paul.patch(&account_url, json!({ "orgUnitId": null })).await;

    let grant = paul
        .post(
            &format!("{account_url}/assignments"),
            json!({
                "roleId": role::TEAM_LEAD,
                "scope": { "type": "org_unit", "orgUnitId": unit_id, "includeDescendants": true },
            }),
        )
        .await;
    assert_eq!(grant.status, StatusCode::CREATED);
    assert_in_use(&paul, &url, "habilitation").await;
    let assignment = grant.json()["id"].as_str().unwrap().to_string();
    paul.delete(&format!("/admin/assignments/{assignment}"))
        .await;

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let memory = create_memory(
        &mut tx,
        NewMemory {
            tenant_id: tenant::ID,
            scope: MemoryScope::OrgUnit,
            owner_user_id: None,
            org_unit_id: Some(unit_id),
            kind: MemoryKind::Lesson,
            title: "Accès au site ouest".into(),
            content: "Clés à retirer à l’accueil.".into(),
            sensitivity: Sensitivity::Internal,
            status: MemoryStatus::Active,
            source: MemorySource::manual(None),
            created_by_user_id: Some(user::PAUL),
            created_by_run_id: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_in_use(&paul, &url, "mémoire").await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    delete_memory(&mut tx, memory).await.unwrap();
    tx.commit().await.unwrap();

    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn checks_parents_types_and_leads() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let orphan = paul
        .post(
            "/admin/org-units",
            json!({ "parentId": Uuid::new_v4(), "name": "Orpheline", "type": "team" }),
        )
        .await;
    assert_eq!(orphan.status, StatusCode::BAD_REQUEST);
    let second_company = paul
        .post(
            "/admin/org-units",
            json!({ "parentId": unit::COMPANY, "name": "Filiale", "type": "company" }),
        )
        .await;
    assert_eq!(second_company.status, StatusCode::BAD_REQUEST);
    let retyped_root = paul
        .patch(
            &format!("/admin/org-units/{}", unit::COMPANY),
            json!({ "type": "team" }),
        )
        .await;
    assert_eq!(retyped_root.status, StatusCode::BAD_REQUEST);
    let unknown = paul
        .patch(
            &format!("/admin/org-units/{}", Uuid::new_v4()),
            json!({ "name": "Fantôme" }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    let moved_to_nowhere = paul
        .patch(
            &format!("/admin/org-units/{}", unit::TEAM_NORTH),
            json!({ "parentId": Uuid::new_v4() }),
        )
        .await;
    assert_eq!(moved_to_nowhere.status, StatusCode::BAD_REQUEST);

    // A lead from another company does not exist from here.
    let north = format!("/admin/org-units/{}", unit::TEAM_NORTH);
    let foreign_lead = paul
        .patch(&north, json!({ "leadUserId": OTHER_USER_ID }))
        .await;
    assert_eq!(foreign_lead.status, StatusCode::BAD_REQUEST);
    let tree = paul.get("/admin/organization").await.json();
    assert_ne!(
        find_unit(&tree, "Équipe Nord").unwrap()["leadUserId"],
        json!(OTHER_USER_ID)
    );
}
