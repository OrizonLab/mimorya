//! What the company's skill evaluations may cost over thirty days
//! (ADR-154): the platform's limit unless those who manage the company's
//! settings set its own, within the platform's bounds, audited; not those
//! who evaluate the skills. Each case played counts what it cost; an
//! evaluation is refused once the limit is reached, and one under way ends
//! there, at the case its played ones reached it. The skill's evaluations
//! are listed with what was spent.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin, holder_of};
use support::llm::SCRIPTED_CALL_MICROS;
use support::skill_evaluations::{
    evaluations, forget, launch, launched, new_version, play, played, status_of, write_case,
};
use support::skill_proposals::restore_the_report;
use support::{Client, TestApi};

const SETTINGS: &str = "/admin/evaluation-settings";
const LATE: &str = "Quels retards cette semaine ?";
const FOLLOW_UPS: &str = "Quels suivis faut-il planifier ?";
const REFUSAL: &str = "Les évaluations de l’entreprise ont coûté 1,00 USD sur les trente \
                       derniers jours, pour une limite de 1,00 USD : ceux qui gèrent ses \
                       réglages peuvent la relever.";

fn limit(max_cost_micros: i64) -> Value {
    json!({ "maxCostMicros": max_cost_micros })
}

async fn set(paul: &Client<'_>, max_cost_micros: i64) {
    let set = paul.put(SETTINGS, limit(max_cost_micros)).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
}

/// Sets what the company's evaluations cost today.
async fn spent_today(api: &TestApi, cost_micros: i64) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "insert into skill_evaluation_spending (tenant_id, day, cost_micros) \
         values ($1, (now() at time zone 'utc')::date, $2) \
         on conflict (tenant_id, day) do update set cost_micros = excluded.cost_micros",
    )
    .bind(tenant::ID)
    .bind(cost_micros)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// The audited changes of the company's limit: action and metadata.
async fn changes(api: &TestApi) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, metadata from audit_log \
         where action like 'skill.evaluation_limit_%' order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// The demo company as the seed left it: the platform's limit, no
/// evaluation nor spending, the weekly report alone.
async fn leave(api: &TestApi, paul: &Client<'_>) {
    assert_eq!(paul.delete(SETTINGS).await.status, StatusCode::OK);
    forget(api, DEMO_SKILL_ID).await;
    restore_the_report(api).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_s_limit_is_set_within_the_platform_s_bounds() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let platform = paul.get(SETTINGS).await.json();
    assert_eq!(
        platform,
        json!({
            "maxCostMicros": 50_000_000, "own": false, "spentMicros": 0, "windowDays": 30,
            "platformMaxCostMicros": 50_000_000, "minCostMicros": 1_000_000,
            "maxCostCeilingMicros": 1_000_000_000,
        })
    );
    let before = changes(&api).await;

    set(&paul, 5_000_000).await;
    spent_today(&api, 1_250_000).await;
    let own = paul.get(SETTINGS).await.json();
    assert_eq!(
        (&own["maxCostMicros"], &own["own"], &own["spentMicros"]),
        (&json!(5_000_000), &json!(true), &json!(1_250_000))
    );
    for refused in [999_999, 1_000_000_001, -1] {
        let put = paul.put(SETTINGS, limit(refused)).await;
        assert_eq!(put.status, StatusCode::BAD_REQUEST, "{refused}");
        assert_eq!(
            put.error_message(),
            "La limite des évaluations sur trente jours va de 1,00 USD à 1000,00 USD."
        );
    }
    let unknown = paul
        .put(SETTINGS, json!({ "maxCostMicros": 5_000_000, "days": 7 }))
        .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert_eq!(paul.get(SETTINGS).await.json()["maxCostMicros"], 5_000_000);

    leave(&api, &paul).await;
    assert_eq!(paul.get(SETTINGS).await.json(), platform);
    assert_eq!(
        changes(&api).await[before.len()..],
        [
            ("skill.evaluation_limit_configured".into(), limit(5_000_000)),
            ("skill.evaluation_limit_reset".into(), json!({})),
        ]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_settings_administrators_set_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.evaluations").await;
    let partial = activate(&api, &mut partial).await;
    // Who evaluates the skills does not raise the limit they spend under.
    let evaluator = holder_of(&api, &paul, "evaluateur.limite", &["skill:manage"]).await;
    for client in [&marc, &partial, &evaluator] {
        assert_eq!(client.get(SETTINGS).await.status, StatusCode::FORBIDDEN);
        let refused = client.put(SETTINGS, limit(1_000_000_000)).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN);
        assert_eq!(client.delete(SETTINGS).await.status, StatusCode::FORBIDDEN);
    }
    assert_eq!(paul.get(SETTINGS).await.json()["own"], false);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_is_refused_once_the_limit_is_reached() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let version = new_version(&paul).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;
    set(&paul, 1_000_000).await;
    spent_today(&api, 1_000_000).await;

    let refused = launch(&paul, DEMO_SKILL_ID, version).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert_eq!(refused.error_message(), REFUSAL);
    let listed = paul
        .get(&format!("/admin/skills/{DEMO_SKILL_ID}/evaluations"))
        .await
        .json();
    assert_eq!(
        (
            &listed["evaluations"],
            &listed["spentMicros"],
            &listed["maxCostMicros"]
        ),
        (&json!([]), &json!(1_000_000), &json!(1_000_000))
    );

    // Raised, it lets the evaluation run.
    set(&paul, 2_000_000).await;
    let mut script = played(false, "Trois retards cette semaine.", true);
    script.extend(played(false, "Trois retards, par technicien.", true));
    api.script(script);
    launched(&paul, DEMO_SKILL_ID, version).await;
    play(&api).await;
    assert_eq!(
        evaluations(&paul, DEMO_SKILL_ID).await[0]["status"],
        "completed"
    );
    leave(&api, &paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn played_cases_count_their_cost_until_the_limit_ends_the_evaluation() {
    let api = TestApi::start_priced().await;
    let paul = api.sign_in(user::PAUL).await;
    let version = new_version(&paul).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;
    write_case(&paul, DEMO_SKILL_ID, "Suivis", FOLLOW_UPS).await;
    set(&paul, 1_000_000).await;
    // Each case, for each version, costs the agent's call and the judge's.
    let case = 2 * SCRIPTED_CALL_MICROS;
    let mut script = Vec::new();
    for _ in 0..3 {
        script.extend(played(false, "Trois retards cette semaine.", true));
    }
    api.script(script);

    let id = launched(&paul, DEMO_SKILL_ID, version).await;
    play(&api).await;
    // 0.40 then 0.80 USD spent: under the limit, the third case is played;
    // 1.20 USD spent before the fourth: the evaluation ends there.
    let (status, failure, results) = status_of(&api, &id).await;
    let reached = "Les évaluations de l’entreprise ont coûté 1,20 USD sur les trente derniers \
                   jours, pour une limite de 1,00 USD : ceux qui gèrent ses réglages peuvent la \
                   relever.";
    assert_eq!(
        (status.as_str(), failure.as_deref(), results),
        ("failed", Some(reached), 3)
    );
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0);
    let spent = paul.get(SETTINGS).await.json()["spentMicros"].clone();
    assert_eq!(spent, json!(3 * case));
    assert_eq!(
        evaluations(&paul, DEMO_SKILL_ID).await[0]["costMicros"],
        json!(3 * case)
    );

    // Reached, it refuses the next one.
    let refused = launch(&paul, DEMO_SKILL_ID, version).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(refused.error_message().contains("1,20 USD"));
    leave(&api, &paul).await;
}
