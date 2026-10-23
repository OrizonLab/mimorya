//! A person's scheduled tasks (ADR-080): created, listed, changed, paused
//! and deleted by their author only, audited without what they ask;
//! another's answers as absent. A task that cannot hold is refused; at most
//! twenty each. Creating or resuming one needs the right to use the agent;
//! pausing or deleting one does not.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::manager_approvals::without;
use support::scheduled::{INSTRUCTION, TASKS, every_day, listed, schedule};
use uuid::Uuid;

fn task_url(id: Uuid) -> String {
    format!("{TASKS}/{id}")
}

fn next_run(task: &Value) -> Option<DateTime<Utc>> {
    task["nextRunAt"].as_str().map(|at| at.parse().unwrap())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_schedules_changes_pauses_and_deletes_their_own_tasks() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let body = json!({
        "title": "  Point\nhebdomadaire ",
        "instruction": INSTRUCTION,
        "weekdays": [5, 1],
        "time": "07:30",
        "timeZone": "Europe/Paris",
    });
    let id = schedule(&marc, body).await;
    let task = listed(&marc, id).await;
    assert_eq!(task["title"], "Point hebdomadaire");
    assert_eq!(task["instruction"], INSTRUCTION);
    assert_eq!(task["weekdays"], json!([1, 5]));
    assert_eq!(task["time"], "07:30");
    assert_eq!(task["enabled"], true);
    assert_eq!(task["lastOccurrence"], Value::Null);
    let next = next_run(&task).expect("runs next");
    assert!(next > Utc::now() && next <= Utc::now() + Duration::days(8));

    // Paused: it runs no more, whatever else changes.
    let paused = marc.patch(&task_url(id), json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let moved = marc.patch(&task_url(id), json!({ "time": "18:00" })).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    let task = listed(&marc, id).await;
    assert_eq!(
        (task["enabled"].clone(), task["time"].clone()),
        (json!(false), json!("18:00"))
    );
    assert_eq!(next_run(&task), None);
    let resumed = marc.patch(&task_url(id), json!({ "enabled": true })).await;
    assert_eq!(resumed.status, StatusCode::OK, "{}", resumed.text());
    assert!(next_run(&listed(&marc, id).await).is_some());

    let deleted = marc.delete(&task_url(id)).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    assert_eq!(listed(&marc, id).await, Value::Null);
    let again = marc.delete(&task_url(id)).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);

    // Audited with when it runs, never what it asks.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: Vec<(String, Option<Uuid>, Value)> = sqlx::query_as(
        "select action, actor_user_id, metadata from audit_log \
         where resource_type = 'scheduled_task' and resource_id = $1::text order by id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let actions: Vec<&str> = audited.iter().map(|(a, _, _)| a.as_str()).collect();
    assert_eq!(
        actions,
        [
            "scheduled_task.created",
            "scheduled_task.updated",
            "scheduled_task.updated",
            "scheduled_task.updated",
            "scheduled_task.deleted",
        ]
    );
    assert!(
        audited
            .iter()
            .all(|(_, actor, _)| *actor == Some(user::MARC))
    );
    assert_eq!(audited[0].2["weekdays"], json!([1, 5]));
    assert!(!format!("{audited:?}").contains("interventions"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nobody_reads_or_changes_another_s_task() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let id = schedule(&marc, every_day("Tâche de Marc")).await;
    assert_eq!(listed(&lucas, id).await, Value::Null);
    let changed = lucas
        .patch(&task_url(id), json!({ "enabled": false }))
        .await;
    assert_eq!(changed.status, StatusCode::NOT_FOUND, "{}", changed.text());
    let deleted = lucas.delete(&task_url(id)).await;
    assert_eq!(deleted.status, StatusCode::NOT_FOUND, "{}", deleted.text());
    // Marc's as it was.
    assert_eq!(listed(&marc, id).await["enabled"], true);
    let removed = marc.delete(&task_url(id)).await;
    assert_eq!(removed.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_task_that_cannot_hold_is_refused_and_twenty_at_most() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let refused = |change: Value, message: &'static str| {
        let nadia = &nadia;
        async move {
            let mut body = every_day("Tâche");
            for (key, value) in change.as_object().unwrap() {
                body[key] = value.clone();
            }
            let answer = nadia.post(TASKS, body).await;
            assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.text());
            assert_eq!(answer.json()["error"]["message"], message);
        }
    };
    let title = "L’intitulé compte de 1 à 120 caractères.";
    refused(json!({ "title": " \u{200B} " }), title).await;
    refused(json!({ "title": "t".repeat(121) }), title).await;
    let instruction = "La demande compte de 1 à 4 000 caractères.";
    refused(json!({ "instruction": "  " }), instruction).await;
    refused(json!({ "instruction": "x".repeat(4001) }), instruction).await;
    refused(
        json!({ "weekdays": [] }),
        "Choisissez au moins un jour de la semaine.",
    )
    .await;
    refused(
        json!({ "weekdays": [8] }),
        "Les jours vont de 1 (lundi) à 7 (dimanche).",
    )
    .await;
    refused(
        json!({ "time": "25:00" }),
        "L’heure s’écrit HH:MM, de 00:00 à 23:59.",
    )
    .await;
    refused(
        json!({ "timeZone": "Lune/Tranquillité" }),
        "Fuseau horaire inconnu.",
    )
    .await;
    let unknown_field = nadia
        .post(TASKS, json!({ "title": "t", "owner": user::MARC }))
        .await;
    assert_eq!(unknown_field.status, StatusCode::BAD_REQUEST);

    let ids = {
        let mut ids = Vec::new();
        for n in 0..20 {
            ids.push(schedule(&nadia, every_day(&format!("Tâche {n}"))).await);
        }
        ids
    };
    let twenty_first = nadia.post(TASKS, every_day("Une de trop")).await;
    assert_eq!(
        twenty_first.status,
        StatusCode::CONFLICT,
        "{}",
        twenty_first.text()
    );
    for id in ids {
        assert_eq!(nadia.delete(&task_url(id)).await.status, StatusCode::OK);
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn running_one_needs_the_agent_but_pausing_or_deleting_does_not() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Avant le retrait")).await;
    let policy = without(&paul, user::MARC, "agent:use").await;

    let created = marc.post(TASKS, every_day("Après le retrait")).await;
    assert_eq!(created.status, StatusCode::FORBIDDEN, "{}", created.text());
    let paused = marc.patch(&task_url(id), json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let resumed = marc.patch(&task_url(id), json!({ "enabled": true })).await;
    assert_eq!(resumed.status, StatusCode::FORBIDDEN, "{}", resumed.text());
    assert_eq!(listed(&marc, id).await["enabled"], false);
    let deleted = marc.delete(&task_url(id)).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());

    let restored = paul.delete(&policy).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
}
