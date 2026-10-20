//! Only an administrator holding `skill:manage` on the whole company reaches
//! its skills (ADR-052). A member is refused on every route, each change
//! audited (ADR-038). Another company's skill answers as one that exists
//! nowhere, on every route, the attempt audited as such (ADR-047), and it is
//! left as it was: no version is ever written for it, although the version
//! table's foreign key would accept its identifier.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::json;
use support::refusals::{latest_entry, not_found_since, refusals_since};
use support::{Client, TestApi, TestResponse};
use uuid::Uuid;

const SKILLS: &str = "/admin/skills";

/// Every change a route makes on skill `id`, deletion last.
async fn every_change(client: &Client<'_>, id: Uuid) -> Vec<(&'static str, TestResponse)> {
    let skill = format!("{SKILLS}/{id}");
    let activation = format!("{skill}/activation");
    vec![
        (
            "POST versions",
            client
                .post(
                    &format!("{skill}/versions"),
                    json!({ "content": "1. Étape." }),
                )
                .await,
        ),
        (
            "PATCH",
            client
                .patch(
                    &skill,
                    json!({ "description": "Une description modifiée." }),
                )
                .await,
        ),
        (
            "PUT activation",
            client.put(&activation, json!({ "version": 1 })).await,
        ),
        ("DELETE activation", client.delete(&activation).await),
        ("DELETE", client.delete(&skill).await),
    ]
}

/// A skill of the other company, a draft without version.
async fn foreign_skill(api: &TestApi) -> Uuid {
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let id = sqlx::query_scalar(
        "insert into skills (tenant_id, name, description) \
         values ($1, 'procedure-ailleurs', 'Une procédure d’une autre entreprise') returning id",
    )
    .bind(OTHER_TENANT_ID)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// The versions of skill `id` a company holds.
async fn versions_of(api: &TestApi, tenant_id: Uuid, id: Uuid) -> i64 {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query_scalar("select count(*) from skill_versions where skill_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_member_is_refused_on_every_route() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let before = paul.get(&format!("{SKILLS}/{DEMO_SKILL_ID}")).await.json();
    let since = latest_entry(&api).await;

    assert_eq!(marc.get(SKILLS).await.status, StatusCode::FORBIDDEN);
    let shown = marc.get(&format!("{SKILLS}/{DEMO_SKILL_ID}")).await;
    assert_eq!(shown.status, StatusCode::FORBIDDEN);
    let created = marc
        .post(
            SKILLS,
            json!({ "name": "relance", "description": "Relancer un client.", "content": "1." }),
        )
        .await;
    assert_eq!(created.status, StatusCode::FORBIDDEN);
    for (route, answer) in every_change(&marc, DEMO_SKILL_ID).await {
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{route}");
    }

    // Each change refused is audited; reads are not.
    assert_eq!(refusals_since(&api, since).await.len(), 6);
    let after = paul.get(&format!("{SKILLS}/{DEMO_SKILL_ID}")).await.json();
    assert_eq!(after, before);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_skill_answers_as_absent_and_stays_as_it_was() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let foreign = foreign_skill(&api).await;
    let since = latest_entry(&api).await;

    for id in [foreign, Uuid::new_v4()] {
        let shown = paul.get(&format!("{SKILLS}/{id}")).await;
        assert_eq!(shown.status, StatusCode::NOT_FOUND);
        for (route, answer) in every_change(&paul, id).await {
            assert_eq!(answer.status, StatusCode::NOT_FOUND, "{route}");
            assert!(answer.text().contains("Compétence introuvable"), "{route}");
        }
    }

    // Each change audited as aimed at a skill answered as absent.
    let absent = not_found_since(&api, since).await;
    assert_eq!(absent.len(), 10);
    for (_, entry) in &absent[..5] {
        assert_eq!(entry.resource, Some(("skill".into(), foreign.to_string())));
    }
    assert!(refusals_since(&api, since).await.is_empty());

    // Untouched, and no version written for it in either company.
    assert_eq!(versions_of(&api, OTHER_TENANT_ID, foreign).await, 0);
    assert_eq!(versions_of(&api, tenant::ID, foreign).await, 0);
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let (status, description): (String, String) =
        sqlx::query_as("select status, description from skills where id = $1")
            .bind(foreign)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(status, "draft");
    assert_eq!(description, "Une procédure d’une autre entreprise");
    sqlx::query("delete from skills where id = $1")
        .bind(foreign)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}
