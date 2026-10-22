//! Lessons drawn from runs (ADR-076): off by default; once the company
//! chose it, a run that used tools is reread and at most one lesson is
//! proposed in the personal memory of whom it served, who alone sees it.
//! Nothing from a run without tools, when the model sees none, or twice.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin};
use support::{Client, TestApi, week};
use uuid::Uuid;

const ANSWER: &str = "Une intervention reste en cours : INT-2049.";

async fn capture_lessons(paul: &Client<'_>, enabled: bool) {
    let body = json!({ "captureLessons": enabled });
    let set = paul.put("/admin/memory-settings", body).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
}

/// A turn of `client` that searches the interventions, then the rereading's
/// steps; the run's identifier.
async fn run_with_tools(api: &TestApi, client: &Client<'_>, reread: Vec<ScriptedStep>) -> Uuid {
    let mut search = week();
    search["status"] = json!(["in_progress"]);
    let mut steps = vec![
        ScriptedStep::calls("", vec![call("interventions_search", search)]),
        ScriptedStep::text(ANSWER),
    ];
    steps.extend(reread);
    api.script(steps);
    ask(api, client).await
}

async fn ask(api: &TestApi, client: &Client<'_>) -> Uuid {
    let conversation = client.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let sent = client
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Quelles interventions restent en cours ?" }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    api.idle().await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0, "steps left");
    sent.json()["runId"].as_str().unwrap().parse().unwrap()
}

fn lesson(title: &str) -> ScriptedStep {
    ScriptedStep::calls(
        "",
        vec![call(
            "submit_lesson",
            json!({
                "aucune": false,
                "titre": title,
                "contenu": "Commencer par les interventions en cours avant de planifier.",
            }),
        )],
    )
}

/// The lessons drawn from runs, as stored: owner, status, title, run.
async fn lessons(api: &TestApi) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select json_build_object('id', id, 'owner', owner_user_id, 'scope', scope, \
         'status', status, 'title', title, 'run', created_by_run_id) \
         from memory_items where kind = 'lesson' and created_by_run_id is not null \
         order by created_at",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// Whether `client` finds the memory `id` among theirs.
async fn listed(client: &Client<'_>, id: &str) -> bool {
    let items = client.get("/memory").await.json()["items"].clone();
    items.as_array().unwrap().iter().any(|m| m["id"] == id)
}

/// The system prompt of the last request to the model.
fn last_system(api: &TestApi) -> String {
    let requests = api.model.as_ref().unwrap().requests();
    requests.last().unwrap().system.clone()
}

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

    let id = drawn[0]["id"].as_str().unwrap();
    let deleted = marc.delete(&format!("/memory/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
    capture_lessons(&paul, false).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_settings_administrators_choose_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let on = json!({ "captureLessons": true });
    assert_eq!(
        marc.get("/admin/memory-settings").await.status,
        StatusCode::FORBIDDEN
    );
    let refused = marc.put("/admin/memory-settings", on.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.lecons").await;
    let partial = activate(&api, &mut partial).await;
    let refused = partial.put("/admin/memory-settings", on.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let unknown = paul
        .put(
            "/admin/memory-settings",
            json!({ "captureLessons": true, "scope": "tenant" }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        paul.get("/admin/memory-settings").await.json()["captureLessons"],
        false
    );

    capture_lessons(&paul, true).await;
    capture_lessons(&paul, false).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: Vec<Value> = sqlx::query_scalar(
        "select metadata from (select id, metadata from audit_log \
         where action = 'memory.lessons_configured' order by id desc limit 2) last \
         order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        audited,
        [
            json!({ "captureLessons": true }),
            json!({ "captureLessons": false })
        ]
    );
}
