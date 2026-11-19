//! What the company pays, shown and weighed everywhere (ADR-212): a run on
//! the operator's key keeps both what its calls cost and what the company
//! was charged for them, the latter shown and told in its usage events; a
//! mission's cost and what its next stage may spend weigh what the company
//! was charged; so do a skill's evaluation and its thirty days' spending;
//! and the person's export counts it.

mod support;

use mimorya_api::services::provisioning::SupplyChange;
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::llm::SCRIPTED_CALL_MICROS;
use support::missions::{
    cost_budget, entrust, executing, limit, read, spend_charged, stages, stop,
};
use support::model_supply::supply;
use support::queue::{API, take_queued};
use support::runs::start_run;
use support::skill_evaluations::{evaluations, forget, launched, play, played, write_case};
use support::{TestApi, owner_url};

/// 10 %: what a call is re-invoiced, beside its cost.
const MARKUP_BPS: u32 = 1_000;
const CHARGED: i64 = SCRIPTED_CALL_MICROS + SCRIPTED_CALL_MICROS / 10;

async fn marked_up(owner: &PgPool, bps: u32) {
    supply(owner, SupplyChange::Markup { bps }).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_keeps_its_cost_and_shows_what_the_company_was_charged() {
    let api = TestApi::start_priced().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    marked_up(&owner, MARKUP_BPS).await;

    api.script(vec![mimorya_agent::testing::ScriptedStep::text(
        "Trois interventions cette semaine.",
    )]);
    let run = start_run(&marc).await;
    api.idle().await;
    // The engine keeps both, for the operator alone.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let (kept, audited): (Value, Value) = sqlx::query_as(
        "select r.usage, a.metadata from agent_runs r join audit_log a on a.run_id = r.id \
         and a.action = 'agent.run.completed' where r.id = $1",
    )
    .bind(run)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(kept["costMicros"], SCRIPTED_CALL_MICROS, "{kept}");
    assert_eq!(kept["chargedMicros"], CHARGED, "{kept}");
    // The company reads what it was charged, in the run, in the list of its
    // runs and in its audit journal; never what the call cost the operator.
    let detail = marc.get(&format!("/runs/{run}")).await.json();
    let list = marc.get("/runs").await.json();
    let listed = list["runs"]
        .as_array()
        .and_then(|runs| runs.iter().find(|item| item["id"] == json!(run)))
        .unwrap_or_else(|| panic!("{list}"))
        .clone();
    for usage in [&detail["run"]["usage"], &listed["usage"], &audited["usage"]] {
        assert_eq!(usage["costMicros"], CHARGED, "{usage}");
        assert!(usage.get("chargedMicros").is_none(), "{usage}");
    }
    // Its usage event tells what the call cost the company.
    let told: Vec<&Value> = detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["type"] == "usage")
        .collect();
    assert_eq!(told.len(), 1, "{detail}");
    assert_eq!(told[0]["data"]["costMicros"], CHARGED, "{detail}");
    marked_up(&owner, 0).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_weighs_what_its_stages_were_charged() {
    let api = TestApi::start().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Mission refacturée").await;
    limit(id, 6, 1_000_000).await;
    // A stage that cost 0.40 USD and was charged 0.50 USD: what is left is
    // what the company was not charged yet.
    spend_charged(id, 400_000, 500_000).await;
    assert_eq!(read(&marc, id).await["costMicros"], 500_000);
    api.script(support::missions::spending_stage("Fait."));
    assert_eq!(take_queued(&api).await, 1);
    let next = stages(id).await.last().unwrap().0;
    assert_eq!(cost_budget(next).await, 500_000);
    assert_eq!(stop(&marc, id).await, axum::http::StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_person_s_export_counts_what_the_company_was_charged() {
    // Without maintenance: its retention would take calls older than five
    // years.
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    // A call on the operator's key, re-invoiced 1.10 USD for 1 USD, and
    // one on the company's own key, which it pays at its cost.
    for (formula, billed) in [("capped", Some(1_100_000_i64)), ("client_key", None)] {
        sqlx::query(
            "insert into usage_records (tenant_id, user_id, kind, role, provider, model, \
             input_tokens, output_tokens, cost_micros, supply_formula, billed_micros, \
             created_at) values ($1, $2, 'llm', 'main', 'anthropic', 'claude-haiku-5-5', 1, 1, \
             1000000, $3, $4, '2020-01-15T12:00:00Z')",
        )
        .bind(tenant::ID)
        .bind(user::MARC)
        .bind(formula)
        .bind(billed)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let exported = marc.get("/account/export").await.json();
    let months: Vec<&Value> = exported["usage"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["month"].as_str().unwrap_or("").starts_with("2020-01"))
        .collect();
    assert_eq!(months.len(), 1, "{exported}");
    assert_eq!(months[0]["costMicros"], json!(2_100_000));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_counts_what_the_company_was_charged() {
    let api = TestApi::start_priced().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let paul = api.sign_in(user::PAUL).await;
    marked_up(&owner, MARKUP_BPS).await;
    let spent = |view: Value| view["spentMicros"].as_i64().unwrap();
    let before = spent(paul.get("/admin/evaluation-settings").await.json());
    write_case(
        &paul,
        DEMO_SKILL_ID,
        "Retards",
        "Quels retards cette semaine ?",
    )
    .await;
    // The active version alone: the agent's answer, then the judge's.
    api.script(played(false, "Trois retards cette semaine.", true));
    launched(&paul, DEMO_SKILL_ID, 1).await;
    play(&api).await;

    assert_eq!(
        evaluations(&paul, DEMO_SKILL_ID).await[0]["costMicros"],
        2 * CHARGED
    );
    let after = spent(paul.get("/admin/evaluation-settings").await.json());
    assert_eq!(after - before, 2 * CHARGED);
    forget(&api, DEMO_SKILL_ID).await;
    marked_up(&owner, 0).await;
}
