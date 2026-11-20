//! A memory the agent saves, and a lesson it proposes, read first by the
//! company's decision service (ADR-213, D3), its main model the engine:
//! active, a personal memory said to hold sensitive data, or one the
//! decision cannot read, is saved as a proposal its owner validates, and the
//! agent is told so; a clear one is saved at once; a lesson said sensitive
//! is not kept, one the decision cannot read is proposed as every lesson
//! is. Observed or off, nothing changes; a team memory, which already
//! awaits a validation, asks nothing.

mod support;

use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::decisions::{DECISIONS, set, settings};
use support::lessons::{capture_lessons, lesson, lessons, run_with_tools};
use support::runs::start_run;
use support::usage_roles::recorded_roles;
use support::{Client, TestApi};

const ANSWER: &str = "C’est noté.";

/// The memory guard set to `mode` at 0.5, by the main model.
fn guarding(mode: &str) -> Value {
    let mut body = settings("mainModel", None);
    body["usages"][2] = json!({ "key": "memoryGuard", "mode": mode, "threshold": 0.5 });
    body
}

/// The main model's answer: health data or not, the rest clear.
fn read(health: bool) -> ScriptedStep {
    let said = |yes: bool| json!({ "reponse": yes, "certitude": "forte" });
    ScriptedStep::text(
        json!({ "sante": said(health), "bancaire": said(false), "personnel_sensible": said(false) })
            .to_string(),
    )
}

fn remember(space: &str, title: &str) -> ScriptedStep {
    ScriptedStep::calls(
        "",
        vec![call(
            "memory_remember",
            json!({ "space": space, "kind": "fact", "title": title,
                    "content": "Contenu de la mémoire de test" }),
        )],
    )
}

/// The status `title` was saved with.
async fn status_of(api: &TestApi, title: &str) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let status = sqlx::query_scalar("select status from memory_items where title = $1")
        .bind(title)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    status
}

/// A run that saves `title` in `space`, the decision answering `decided`.
async fn saved(
    api: &TestApi,
    marc: &Client<'_>,
    space: &str,
    title: &str,
    decided: Option<ScriptedStep>,
) -> Vec<String> {
    let mut steps = vec![remember(space, title)];
    steps.extend(decided);
    steps.push(ScriptedStep::text(ANSWER));
    api.script(steps);
    let run = start_run(marc).await;
    api.idle().await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0, "steps left");
    recorded_roles(api, run).await
}

/// What the tool told the model of the memory it saved, last.
fn told(api: &TestApi) -> String {
    let requests = api.model.as_ref().unwrap().requests();
    format!("{:?}", requests.last().unwrap().messages.last().unwrap())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_personal_memory_said_sensitive_waits_for_its_owner() {
    let api = TestApi::start_priced().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    set(&paul, guarding("active")).await;

    let roles = saved(
        &api,
        &marc,
        "personal",
        "Arrêt de travail",
        Some(read(true)),
    )
    .await;
    assert_eq!(roles, ["main", "decision", "main"]);
    assert_eq!(status_of(&api, "Arrêt de travail").await, "proposed");
    assert!(told(&api).contains("proposé"), "{}", told(&api));

    saved(
        &api,
        &marc,
        "personal",
        "Rapport du lundi",
        Some(read(false)),
    )
    .await;
    assert_eq!(status_of(&api, "Rapport du lundi").await, "active");

    // The decision unread: held for validation too.
    let unread = Some(ScriptedStep::text("Je ne sais pas."));
    saved(&api, &marc, "personal", "Mémoire illisible", unread).await;
    assert_eq!(status_of(&api, "Mémoire illisible").await, "proposed");

    // A team memory already awaits a validation: nothing asked.
    let roles = saved(&api, &marc, "team", "Consigne d’équipe", None).await;
    assert_eq!(roles, ["main", "main"]);
    assert_eq!(status_of(&api, "Consigne d’équipe").await, "proposed");

    // Observed: asked and counted, saved as without it.
    set(&paul, guarding("observe")).await;
    let roles = saved(&api, &marc, "personal", "Observée", Some(read(true))).await;
    assert_eq!(roles, ["main", "decision", "main"]);
    assert_eq!(status_of(&api, "Observée").await, "active");

    // Off: nothing asked.
    assert_eq!(paul.delete(DECISIONS).await.status.as_u16(), 200);
    let roles = saved(&api, &marc, "personal", "Sans décision", None).await;
    assert_eq!(roles, ["main", "main"]);
    assert_eq!(status_of(&api, "Sans décision").await, "active");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_lesson_said_sensitive_is_not_kept() {
    let api = TestApi::start_priced().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    capture_lessons(&paul, true).await;
    set(&paul, guarding("active")).await;
    let titled = |title: &str| {
        let title = title.to_string();
        move |stored: &Value| stored["title"] == title.as_str()
    };

    run_with_tools(&api, &marc, vec![lesson("Leçon sensible"), read(true)]).await;
    assert!(!lessons(&api).await.iter().any(titled("Leçon sensible")));

    // The decision unread: proposed, as every lesson is.
    let unread = ScriptedStep::text("Je ne sais pas.");
    run_with_tools(&api, &marc, vec![lesson("Leçon illisible"), unread]).await;
    let kept = lessons(&api).await;
    let stored = kept
        .iter()
        .find(|l| titled("Leçon illisible")(l))
        .expect("kept");
    assert_eq!(stored["status"], "proposed");

    run_with_tools(&api, &marc, vec![lesson("Leçon claire"), read(false)]).await;
    assert!(lessons(&api).await.iter().any(titled("Leçon claire")));
    assert_eq!(paul.delete(DECISIONS).await.status.as_u16(), 200);
    capture_lessons(&paul, false).await;
}
