//! Memory rights through the API: personal memory stays personal.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;

fn find<'a>(items: &'a [Value], title: &str) -> &'a Value {
    items
        .iter()
        .find(|i| i["title"] == title)
        .unwrap_or_else(|| panic!("memory {title} not found"))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn lets_users_see_and_erase_their_own_memory_never_other_peoples() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let marc = api.sign_in(user::MARC).await;
    let items = lucas.get("/memory").await.json()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!items.iter().any(|i| i["title"] == "Format des synthèses"));
    let own = find(&items, "Prévenance des déplacements");
    assert_eq!(own["canManage"], true);
    let own_url = format!("/memory/{}", own["id"].as_str().unwrap());

    assert_eq!(marc.delete(&own_url).await.status, StatusCode::NOT_FOUND);
    assert_eq!(lucas.delete(&own_url).await.status, StatusCode::OK);
    let after = lucas.get("/memory").await.json();
    assert!(
        !after["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == own["id"])
    );

    let shared = find(&items, "Retours chantier équipe Nord");
    assert_eq!(shared["canManage"], false);
    let shared_url = format!("/memory/{}", shared["id"].as_str().unwrap());
    assert_eq!(
        lucas.delete(&shared_url).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn writes_only_into_spaces_the_user_may_write() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let entry = |space: &str| {
        json!({
            "space": space,
            "kind": "preference",
            "title": "  Format court  ",
            "content": "Réponses en trois points maximum.",
        })
    };
    let personal = lucas.post("/memory", entry("personal")).await;
    assert_eq!(personal.status, StatusCode::CREATED);
    let company = lucas.post("/memory", entry("company")).await;
    assert_eq!(company.status, StatusCode::FORBIDDEN);

    let items = lucas.get("/memory").await.json();
    let created = items["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == personal.json()["id"])
        .cloned()
        .expect("created memory listed");
    assert_eq!(created["title"], "Format court");
    assert_eq!(created["scope"], "user");

    let invalid = lucas
        .post(
            "/memory",
            json!({ "space": "personal", "kind": "preference", "title": "x", "content": "y" }),
        )
        .await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
}
