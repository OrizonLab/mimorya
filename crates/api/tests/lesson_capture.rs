//! Lessons drawn from runs (ADR-076): off by default; once the company
//! chose it, a run that used tools is reread and at most one lesson is
//! proposed in the personal memory of whom it served, who alone sees it,
//! even when a manager approved the run's action. Nothing from a run without
//! tools, when the model sees none, or twice, a refused one included; a
//! failed rereading loses the lesson, never the run.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::lessons::{
    ask, capture_lessons, last_system, lesson, lessons, listed, run_with_tools,
};
use support::manager_approvals::{applying, route_to_managers};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_lesson_is_proposed_to_whom_the_run_served_once_the_company_chose_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let lucas = api.sign_in(user::LUCAS).await;

    // Off by default: the run is reread by nobody.
    let shown = paul.get("/admin/memory-settings").await.json();
    assert_eq!(shown, json!({ "captureLessons": false }));
    run_with_tools(&api, &marc, vec![]).await;
    assert!(!last_system(&api).starts_with("Tu relis"));
    assert!(lessons(&api).await.is_empty());

    capture_lessons(&paul, true).await;
    let shown = paul.get("/admin/memory-settings").await.json();
    assert_eq!(shown["captureLessons"], true);
    let run = run_with_tools(&api, &marc, vec![lesson("Interventions en cours d’abord")]).await;
    assert!(last_system(&api).starts_with("Tu relis une conversation terminée"));
    let drawn = lessons(&api).await;
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert_eq!(drawn[0]["owner"], json!(user::MARC));
    assert_eq!(drawn[0]["scope"], "user");
    assert_eq!(drawn[0]["status"], "proposed");
    assert_eq!(drawn[0]["run"], json!(run));
    let id = drawn[0]["id"].as_str().unwrap().to_owned();

    // Marc's alone: Lucas never sees it.
    assert!(listed(&marc, &id).await);
    assert!(!listed(&lucas, &id).await);
    // Audited without its text; its cost counted in the run.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: (Option<Uuid>, Option<Uuid>, Value) = sqlx::query_as(
        "select on_behalf_of_user_id, run_id, metadata from audit_log \
         where action = 'memory.lesson_proposed' and resource_id = $1",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(audited.0, Some(user::MARC));
    assert_eq!(audited.1, Some(run));
    assert!(!audited.2.to_string().contains("Commencer"));
    let calls: i64 =
        sqlx::query_scalar("select count(*) from usage_records where run_id = $1 and kind = 'llm'")
            .bind(run)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(calls, 3);
    drop(tx);

    // Approved, it is a lesson he keeps.
    let review = format!("/memory/{id}/review");
    let approved = marc.post(&review, json!({ "decision": "approve" })).await;
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.text());
    assert_eq!(lessons(&api).await[0]["status"], "active");

    let deleted = marc.delete(&format!("/memory/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
    capture_lessons(&paul, false).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_from_a_run_without_tools_when_none_is_seen_or_twice() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    capture_lessons(&paul, true).await;

    // A mere exchange is not reread.
    api.script(vec![ScriptedStep::text("Bonjour Marc.")]);
    ask(&api, &marc).await;
    assert!(!last_system(&api).starts_with("Tu relis"));

    // Nothing worth keeping.
    let none = ScriptedStep::calls("", vec![call("submit_lesson", json!({ "aucune": true }))]);
    run_with_tools(&api, &marc, vec![none]).await;
    assert!(last_system(&api).starts_with("Tu relis"));
    assert!(lessons(&api).await.is_empty());

    // Once, then named to the model and never twice.
    run_with_tools(&api, &marc, vec![lesson("Interventions en cours d’abord")]).await;
    run_with_tools(&api, &marc, vec![lesson("interventions en cours D’ABORD")]).await;
    let requests = api.model.as_ref().unwrap().requests();
    let Some(mimorya_agent::llm::LlmMessage::User { content }) =
        requests.last().unwrap().messages.first()
    else {
        panic!("one user message");
    };
    assert!(
        content.contains("- Interventions en cours d’abord"),
        "{content}"
    );
    let drawn = lessons(&api).await;
    assert_eq!(drawn.len(), 1, "{drawn:?}");

    // Refused, it does not come back either.
    let id = drawn[0]["id"].as_str().unwrap().to_owned();
    let review = format!("/memory/{id}/review");
    let rejected = marc.post(&review, json!({ "decision": "reject" })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    run_with_tools(&api, &marc, vec![lesson("Interventions en cours d’abord")]).await;
    let drawn = lessons(&api).await;
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert_eq!(drawn[0]["status"], "archived");

    let deleted = marc.delete(&format!("/memory/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
    capture_lessons(&paul, false).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_lesson_goes_to_the_requester_even_once_a_manager_approved() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let nadia = api.sign_in(user::NADIA).await;
    capture_lessons(&paul, true).await;
    route_to_managers(&paul, &["write"]).await;
    let (run, approval) = applying(&api, &nadia, ("INT-2049", "09:00"), "Planifié.").await;
    route_to_managers(&paul, &[]).await;
    let approval = approval.expect("a request awaiting her manager");

    // Resumed on Marc's decision, with Nadia's access: hers to keep.
    api.script(vec![lesson("Vérifier le créneau avant d’appliquer")]);
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let approved = marc.post(&decision, json!({ "approved": true })).await;
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.text());
    api.idle().await;
    let drawn = lessons(&api).await;
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert_eq!(drawn[0]["owner"], json!(user::NADIA));
    assert_eq!(drawn[0]["run"].as_str(), Some(run.as_str()));
    let id = drawn[0]["id"].as_str().unwrap();
    assert!(listed(&nadia, id).await);
    assert!(!listed(&marc, id).await);

    let deleted = nadia.delete(&format!("/memory/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
    capture_lessons(&paul, false).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_failed_rereading_loses_the_lesson_never_the_run() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    capture_lessons(&paul, true).await;
    // No answer scripted for the rereading: the model fails it.
    let run = run_with_tools(&api, &marc, vec![]).await;
    assert!(last_system(&api).starts_with("Tu relis"));
    assert!(lessons(&api).await.is_empty());
    let shown = marc.get(&format!("/runs/{run}")).await.json();
    assert_eq!(shown["run"]["status"], "completed", "{shown}");
    capture_lessons(&paul, false).await;
}
