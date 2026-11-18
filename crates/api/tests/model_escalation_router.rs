//! The router of a company's model escalation (ADR-207, step 2): its rule
//! for each kind of run is kept, read back and audited, and a route it does
//! not know is refused; a turn of conversation, a scheduled task's run, a
//! hook's and a mission's stage each start on the model their own rule
//! names; a classification of the request by
//! the main model is recorded as such, its cost counted in the month's
//! escalation spending; a skill version asks for the escalation model, the
//! next one written without saying keeps that, and a run that loads it
//! goes on with that model.

mod support;

use axum::http::StatusCode;
use chrono::Duration;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::hooks::{deliver, hook};
use support::llm::{API_KEY, ENDPOINT, ESCALATION_MODEL, MASTER_KEY, SCRIPTED_CALL_MICROS};
use support::missions::{approving_review, entrust, executing, read};
use support::queue::take_queued;
use support::refusals::{entries_since, latest_entry};
use support::runs::start_run;
use support::scheduled::{due, every_day, fire, schedule};
use support::usage_roles::recorded_roles;
use support::{Client, TestApi};
use uuid::Uuid;

const ESCALATION: &str = "/admin/model-escalation";
const SKILLS: &str = "/admin/skills";
const ANSWER: &str = "Trois interventions cette semaine.";

const ENV: [(&str, &str); 2] = [
    ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
];

async fn start() -> TestApi {
    TestApi::start_with(&ENV).await
}

fn routes(conversation: &str, mission: &str, scheduled_task: &str, hook: &str) -> Value {
    json!({
        "conversation": conversation,
        "mission": mission,
        "scheduledTask": scheduled_task,
        "hook": hook,
    })
}

fn settings(routes: Value) -> Value {
    json!({
        "model": ESCALATION_MODEL,
        "afterFailure": false,
        "router": true,
        "advisor": false,
        "routes": routes,
        "monthlyBudgetMicros": 50_000_000,
        "maxPerRun": 5,
    })
}

/// The company on its own provider, its router on with `routes`.
async fn routing(paul: &Client<'_>, routes: Value) {
    let own = json!({ "provider": "anthropic", "model": "claude-haiku-5-5", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", own).await.status, StatusCode::OK);
    let chosen = paul.put(ESCALATION, settings(routes)).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
}

async fn leave(paul: &Client<'_>) {
    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_rule_of_each_kind_of_run_is_kept_read_back_and_audited() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    routing(
        &paul,
        routes("escalation", "classification", "main", "escalation"),
    )
    .await;
    let shown = paul.get(ESCALATION).await.json();
    assert_eq!(shown["settings"]["router"], true);
    assert_eq!(
        shown["settings"]["routes"],
        routes("escalation", "classification", "main", "escalation")
    );
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_escalation.configured"), "{audit}");
    assert!(audit.contains("classification"), "{audit}");

    // A route the router does not know, or none: refused, nothing changed.
    let unknown = settings(routes("escalation", "haut", "main", "main"));
    let mut missing = settings(Value::Null);
    missing.as_object_mut().unwrap().remove("routes");
    let mut partial = settings(routes("main", "main", "main", "main"));
    partial["routes"].as_object_mut().unwrap().remove("hook");
    for body in [unknown, missing, partial] {
        let refused = paul.put(ESCALATION, body.clone()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}");
    }
    assert_eq!(
        paul.get(ESCALATION).await.json()["settings"]["routes"]["mission"],
        "classification"
    );
    leave(&paul).await;
}

/// The run the hook or scheduled task `id` of `table` started last.
async fn last_run(api: &TestApi, table: &str, id: Uuid) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(&format!("select last_run_id from {table} where id = $1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_kind_of_run_starts_on_the_model_its_rule_names() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    routing(
        &paul,
        routes("main", "main", "classification", "escalation"),
    )
    .await;
    let since = latest_entry(&api).await;

    // A turn of conversation keeps the main model.
    api.company_model.push([ScriptedStep::text(ANSWER)]);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(recorded_roles(&api, run).await, ["main"]);

    // A scheduled task's run is classified first.
    let task = schedule(&marc, every_day("Point du matin")).await;
    api.company_model.push([
        ScriptedStep::text(r#"{"niveau": "principal"}"#),
        ScriptedStep::text(ANSWER),
    ]);
    due(task, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 1);
    let run = last_run(&api, "scheduled_tasks", task).await;
    assert_eq!(recorded_roles(&api, run).await, ["classification", "main"]);
    assert!(api.escalation_model.requests().is_empty());

    // A hook's run starts on the escalation model.
    let (id, token) = hook(&marc, "Pannes signalées").await;
    api.escalation_model.push([ScriptedStep::text(ANSWER)]);
    let accepted = deliver(&api, id, &token, r#"{"panne": "pompe"}"#).await;
    assert_eq!(accepted.status, StatusCode::ACCEPTED, "{}", accepted.text());
    api.idle().await;
    let run = last_run(&api, "inbound_hooks", id).await;
    assert_eq!(recorded_roles(&api, run).await, ["escalation"]);
    assert_eq!(api.escalation_model.requests().len(), 1);
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_escalation.started"), "{audit}");
    assert!(audit.contains("rule"), "{audit}");

    for removed in [
        format!("/inbound-hooks/{id}"),
        format!("/scheduled-tasks/{task}"),
    ] {
        assert_eq!(marc.delete(&removed).await.status, StatusCode::OK);
    }
    leave(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_s_stage_starts_on_the_model_its_rule_names() {
    let api = start().await;
    executing(&api);
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    routing(&paul, routes("main", "escalation", "main", "main")).await;

    let id = entrust(&marc, "Suivi de la semaine").await;
    api.escalation_model.push([ScriptedStep::text(ANSWER)]);
    // Reviewed on the mission's criteria by the main model.
    api.company_model.push([approving_review()]);
    assert_eq!(take_queued(&api).await, 1);
    assert_eq!(read(&marc, id).await["status"], "done");
    let stage: Uuid = {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        sqlx::query_scalar("select id from agent_runs where mission_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap()
    };
    assert_eq!(recorded_roles(&api, stage).await, ["escalation", "review"]);
    leave(&paul).await;
}

/// What the run's calls of `role` cost.
async fn cost_of(api: &TestApi, run: Uuid, role: &str) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select coalesce(sum(cost_micros), 0)::bigint from usage_records \
         where run_id = $1 and role = $2",
    )
    .bind(run)
    .bind(role)
    .fetch_one(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_classification_is_recorded_and_counted_in_the_month_s_spending() {
    // Each call of the scripted model costs `SCRIPTED_CALL_MICROS`.
    let api = TestApi::start_priced_with(&ENV).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    routing(&paul, routes("classification", "main", "main", "main")).await;
    let spent = |view: Value| view["spentMicros"].as_i64().unwrap();
    let before = spent(paul.get(ESCALATION).await.json());

    api.company_model
        .push([ScriptedStep::text(r#"{"niveau": "escalade"}"#)]);
    api.escalation_model.push([ScriptedStep::text(ANSWER)]);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(
        recorded_roles(&api, run).await,
        ["classification", "escalation"]
    );
    let classified = cost_of(&api, run, "classification").await;
    let escalated = cost_of(&api, run, "escalation").await;
    assert_eq!(
        (classified, escalated),
        (SCRIPTED_CALL_MICROS, SCRIPTED_CALL_MICROS)
    );
    assert_eq!(
        spent(paul.get(ESCALATION).await.json()) - before,
        2 * SCRIPTED_CALL_MICROS
    );
    leave(&paul).await;
}

fn analysis(model_escalation: bool) -> Value {
    json!({
        "name": "analyse-approfondie",
        "description": "Analyse approfondie des interventions d’un site.",
        "content": "1. Croiser les interventions et leurs comptes rendus.",
        "modelEscalation": model_escalation,
    })
}

async fn versions(paul: &Client<'_>, id: &str) -> Vec<(i64, bool)> {
    let detail = paul.get(&format!("{SKILLS}/{id}")).await.json();
    detail["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["version"].as_i64().unwrap(),
                v["modelEscalation"].as_bool().unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_skill_version_that_asks_for_the_escalation_model_switches_the_run_loading_it() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    routing(&paul, routes("main", "main", "main", "main")).await;

    let created = paul.post(SKILLS, analysis(true)).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_owned();
    let added = |body: Value| {
        let (paul, path) = (&paul, format!("{SKILLS}/{id}/versions"));
        async move {
            let added = paul.post(&path, body).await;
            assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
        }
    };
    let activation = format!("{SKILLS}/{id}/activation");
    let active = paul.put(&activation, json!({ "version": 1 })).await;
    assert_eq!(active.status, StatusCode::OK, "{}", active.text());
    // Written without saying: as the version active does; or said.
    added(json!({ "content": "1. Croiser, puis conclure." })).await;
    added(json!({ "content": "1. Lister.", "modelEscalation": false })).await;
    assert_eq!(
        versions(&paul, &id).await,
        [(3, false), (2, true), (1, true)]
    );

    let load = call("skill_read", json!({ "name": "analyse-approfondie" }));
    api.company_model
        .push([ScriptedStep::calls("", vec![load])]);
    api.escalation_model.push([ScriptedStep::text(ANSWER)]);
    let run = start_run(&marc).await;
    api.idle().await;
    assert_eq!(recorded_roles(&api, run).await, ["main", "escalation"]);

    paul.delete(&activation).await;
    let deleted = paul.delete(&format!("{SKILLS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    leave(&paul).await;
}
