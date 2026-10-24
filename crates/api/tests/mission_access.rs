//! Who may entrust, read and stop a mission (ADR-089): its owner alone,
//! anyone else's answering as absent, even to an administrator; entrusting
//! one needs `mission:create`, criteria within bounds, and fewer than three
//! missions under way; following them, `mission:read`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::json;
use support::missions::{entrust, mission_body};
use support::{Client, TestApi};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn someone_else_s_mission_answers_as_absent() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Mission de Marc").await;
    for other in [user::CLAIRE, user::PAUL] {
        let other = api.sign_in(other).await;
        let read = other.get(&format!("/missions/{id}")).await;
        assert_eq!(read.status, StatusCode::NOT_FOUND);
        let stopped = other.post(&format!("/missions/{id}/stop"), json!({})).await;
        assert_eq!(stopped.status, StatusCode::NOT_FOUND);
        let listed = other.get("/missions").await.json();
        assert_eq!(listed["missions"], json!([]));
    }
    let unknown = marc.get(&format!("/missions/{}", Uuid::new_v4())).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    // Its owner's list holds it, still under way.
    let listed = marc.get("/missions").await.json();
    assert_eq!(listed["missions"][0]["id"], id.to_string());
    assert_eq!(listed["missions"][0]["status"], "in_progress");
    assert_eq!(listed["maxUnderWay"], 3);
}

/// Denies Marc `permissions`, as Paul: the policy's address.
async fn deny_marc(paul: &Client<'_>, permissions: &[&str]) -> String {
    let marc_only = json!({
        "op": "eq",
        "left": { "attr": "subject.userId" },
        "right": { "value": user::MARC.to_string() }
    });
    let body = json!({ "description": "Missions refusées", "permissions": permissions, "condition": marc_only });
    let created = paul.post("/admin/policies", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    format!("/admin/policies/{}", created.json()["id"].as_str().unwrap())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn entrusting_and_following_missions_need_their_rights() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Mission suivie").await;

    let denied = deny_marc(&paul, &["mission:create"]).await;
    let refused = marc
        .post("/missions", mission_body("Mission refusée"))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    // Following the one entrusted needs another right.
    assert_eq!(
        marc.get(&format!("/missions/{id}")).await.status,
        StatusCode::OK
    );
    assert_eq!(paul.delete(&denied).await.status, StatusCode::OK);

    let denied = deny_marc(&paul, &["mission:read"]).await;
    assert_eq!(marc.get("/missions").await.status, StatusCode::FORBIDDEN);
    let read = marc.get(&format!("/missions/{id}")).await;
    assert_eq!(read.status, StatusCode::FORBIDDEN);
    let stopped = marc.post(&format!("/missions/{id}/stop"), json!({})).await;
    assert_eq!(stopped.status, StatusCode::FORBIDDEN);
    assert_eq!(paul.delete(&denied).await.status, StatusCode::OK);
    let listed = marc.get("/missions").await.json();
    assert_eq!(listed["missions"].as_array().unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_states_its_criteria_and_nothing_else() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let mut no_criteria = mission_body("Sans critère");
    no_criteria["successCriteria"] = json!(["  "]);
    let refused = marc.post("/missions", no_criteria).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    let mut unknown = mission_body("Champ inconnu");
    unknown["budget"] = json!(1000);
    let refused = marc.post("/missions", unknown).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    let listed = marc.get("/missions").await.json();
    assert_eq!(listed["missions"], json!([]));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn three_missions_at_most_are_under_way_at_once() {
    let api = TestApi::start().await;
    // Hugo entrusts none elsewhere in these tests.
    let hugo = api.sign_in(user::HUGO).await;
    let first = entrust(&hugo, "Première").await;
    entrust(&hugo, "Deuxième").await;
    entrust(&hugo, "Troisième").await;
    let fourth = hugo.post("/missions", mission_body("Quatrième")).await;
    assert_eq!(fourth.status, StatusCode::CONFLICT);
    // One stopped, another may be entrusted.
    let stopped = hugo
        .post(&format!("/missions/{first}/stop"), json!({}))
        .await;
    assert_eq!(stopped.status, StatusCode::OK);
    entrust(&hugo, "Quatrième").await;
    let listed = hugo.get("/missions").await.json();
    assert_eq!(listed["missions"].as_array().unwrap().len(), 4);
}
