//! A company's own mission limits (ADR-093): chosen by its company-wide
//! settings administrators only, within the platform's bounds, audited. A
//! mission entrusted keeps the limits of the day: a later change, or a
//! return to the platform's, leaves it as it was. Another company's limits
//! are its own.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin};
use support::missions::{cost_budget, entrust, read, stages, stop};
use support::{Client, TestApi, owner_url};

const SETTINGS: &str = "/admin/mission-settings";

fn limits(max_stages: u32, max_cost_micros: i64) -> Value {
    json!({ "maxStages": max_stages, "maxCostMicros": max_cost_micros })
}

async fn set(paul: &Client<'_>, max_stages: u32, max_cost_micros: i64) {
    let set = paul
        .put(SETTINGS, limits(max_stages, max_cost_micros))
        .await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
}

/// The audited changes of the demo company's limits: action and metadata.
async fn changes(api: &TestApi) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, metadata from audit_log where action like 'mission.limits_%' \
         order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_keeps_the_company_s_limits_of_the_day() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let platform = paul.get(SETTINGS).await.json();
    assert_eq!(
        platform,
        json!({
            "maxStages": 6, "maxCostMicros": 12_000_000, "own": false,
            "platformMaxStages": 6, "platformMaxCostMicros": 12_000_000,
            "maxStagesCeiling": 20, "minCostMicros": 1_000_000,
            "maxCostCeilingMicros": 100_000_000,
        })
    );

    set(&paul, 3, 2_000_000).await;
    let own = paul.get(SETTINGS).await.json();
    assert_eq!(
        (own["maxStages"].clone(), own["own"].clone()),
        (3.into(), true.into())
    );
    let kept = entrust(&marc, "Mission aux limites de l’entreprise").await;
    let mission = read(&marc, kept).await;
    assert_eq!(
        (
            mission["maxStages"].clone(),
            mission["maxCostMicros"].clone()
        ),
        (3.into(), 2_000_000.into())
    );
    // Its first stage may spend no more than the mission.
    assert_eq!(cost_budget(stages(kept).await[0].0).await, 2_000_000);

    // Changed, then back to the platform's: the mission keeps its own.
    set(&paul, 10, 20_000_000).await;
    let later = entrust(&marc, "Mission entrustée après le changement").await;
    assert_eq!(read(&marc, later).await["maxStages"], 10);
    let reset = paul.delete(SETTINGS).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(paul.get(SETTINGS).await.json(), platform);
    assert_eq!(read(&marc, kept).await["maxStages"], 3);
    assert_eq!(read(&marc, later).await["maxStages"], 10);
    let after = entrust(&marc, "Mission aux limites de la plateforme").await;
    assert_eq!(read(&marc, after).await["maxCostMicros"], 12_000_000);

    let changed = changes(&api).await;
    assert_eq!(
        changed[changed.len() - 3..],
        [
            ("mission.limits_configured".into(), limits(3, 2_000_000)),
            ("mission.limits_configured".into(), limits(10, 20_000_000)),
            ("mission.limits_reset".into(), json!({})),
        ]
    );
    for id in [kept, later, after] {
        assert_eq!(stop(&marc, id).await, StatusCode::OK);
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn limits_beyond_the_platform_s_bounds_are_refused() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let before = changes(&api).await;
    for (stages, cost) in [
        (0, 12_000_000),
        (21, 12_000_000),
        (6, 999_999),
        (6, 100_000_001),
        (6, -1),
    ] {
        let refused = paul.put(SETTINGS, limits(stages, cost)).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{stages} {cost}");
        assert_eq!(
            refused.error_message(),
            "Une mission compte de 1 à 20 étapes au plus, et coûte de 1 à 100 USD au plus."
        );
    }
    let unknown = paul
        .put(
            SETTINGS,
            json!({ "maxStages": 6, "maxCostMicros": 12_000_000, "maxUnderWay": 9 }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    let negative = paul
        .put(
            SETTINGS,
            json!({ "maxStages": -1, "maxCostMicros": 12_000_000 }),
        )
        .await;
    assert_eq!(negative.status, StatusCode::BAD_REQUEST);
    assert_eq!(paul.get(SETTINGS).await.json()["own"], false);
    assert_eq!(changes(&api).await, before);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_settings_administrators_set_them() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.missions").await;
    let partial = activate(&api, &mut partial).await;
    for client in [&marc, &partial] {
        assert_eq!(client.get(SETTINGS).await.status, StatusCode::FORBIDDEN);
        let refused = client.put(SETTINGS, limits(20, 100_000_000)).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN);
        assert_eq!(client.delete(SETTINGS).await.status, StatusCode::FORBIDDEN);
    }
    assert_eq!(paul.get(SETTINGS).await.json()["own"], false);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_limits_are_its_own() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "insert into tenant_mission_settings (tenant_id, max_stages, max_cost_micros) \
         values ($1, 20, 100000000)",
    )
    .bind(OTHER_TENANT_ID)
    .execute(&owner)
    .await
    .unwrap();

    let read = paul.get(SETTINGS).await.json();
    assert_eq!(
        (read["maxStages"].clone(), read["own"].clone()),
        (6.into(), false.into())
    );
    // Back to its platform's limits, the demo company leaves the other's.
    assert_eq!(paul.delete(SETTINGS).await.status, StatusCode::OK);
    let kept: i64 =
        sqlx::query_scalar("select count(*) from tenant_mission_settings where tenant_id = $1")
            .bind(OTHER_TENANT_ID)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(kept, 1);
    sqlx::query("delete from tenant_mission_settings where tenant_id = $1")
        .bind(OTHER_TENANT_ID)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}
