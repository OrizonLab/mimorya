//! The skills a run lists, chosen by the company's decision service
//! (ADR-213, D2), its main model the engine: active, the system prompt lists
//! none and the first turn lists those said useful, or every one when the
//! decision fails; observed, the decision is asked and counted, the system
//! prompt lists them all as before and the first turn none; a sub-agent is
//! listed what its parent was, even one entrusted once the run resumes
//! after a validation; a person whose agent may not read skills is listed
//! none, and nothing is asked. (The
//! rules of the choice itself: `crates/agent/src/runtime/skill_choice.rs`.)

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::skills::list_active_skills;
use serde_json::{Map, Value, json};
use support::decisions::{DECISIONS, set, settings};
use support::llm::SCRIPTED_CALL_MICROS;
use support::planning::{apply_proposal, propose};
use support::runs::start_run;
use support::usage_roles::recorded_roles;
use support::{Client, TestApi};
use uuid::Uuid;

const ANSWER: &str = "Trois interventions cette semaine.";
/// The heading of the full index, in the system prompt.
const FULL_INDEX: &str = "## Compétences de l’entreprise\n";
/// The heading of the skills the first turn lists.
const CHOSEN: &str = "## Compétences de l’entreprise utiles à cette demande";

/// The skill choice set to `mode` at 0.8, by the main model.
fn choosing(mode: &str) -> Value {
    let mut body = settings("mainModel", None);
    body["usages"][1] = json!({ "key": "skillChoice", "mode": mode, "threshold": 0.8 });
    body
}

/// The company's active skills, by name, in the order they are asked about.
async fn skill_names(api: &TestApi) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let skills = list_active_skills(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    skills.into_iter().map(|skill| skill.name).collect()
}

/// The main model's answer: the first skill useful, the others not.
fn first_useful(count: usize) -> ScriptedStep {
    let answers: Map<String, Value> = (0..count)
        .map(|at| {
            let said = json!({ "reponse": at == 0, "certitude": "forte" });
            (format!("competence_{}", at + 1), said)
        })
        .collect();
    ScriptedStep::text(Value::Object(answers).to_string())
}

/// The system prompt and first turn of the run's answer.
fn answered_with(api: &TestApi) -> (String, String) {
    let requests = api.model.as_ref().unwrap().requests();
    let last = requests.last().unwrap();
    let turn = format!("{:?}", last.messages.last().unwrap());
    (last.system.clone(), turn)
}

async fn run(api: &TestApi, marc: &Client<'_>, steps: Vec<ScriptedStep>) -> Vec<String> {
    run_with_id(api, marc, steps).await.1
}

async fn run_with_id(
    api: &TestApi,
    marc: &Client<'_>,
    steps: Vec<ScriptedStep>,
) -> (Uuid, Vec<String>) {
    api.script(steps);
    let run = start_run(marc).await;
    api.idle().await;
    (run, recorded_roles(api, run).await)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_skills_a_run_lists_are_chosen_by_the_decision() {
    let api = TestApi::start_priced().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let names = skill_names(&api).await;
    assert!(!names.is_empty(), "the demo company has skills");
    let answer = || ScriptedStep::text(ANSWER);

    // Active: the first skill alone, with the request.
    set(&paul, choosing("active")).await;
    let (id, roles) = run_with_id(&api, &marc, vec![first_useful(names.len()), answer()]).await;
    assert_eq!(roles, ["decision", "main"]);
    // The decision's cost in the run's own.
    let detail = marc.get(&format!("/runs/{id}")).await.json();
    assert_eq!(
        detail["run"]["usage"]["costMicros"],
        2 * SCRIPTED_CALL_MICROS,
        "{detail}"
    );
    let (system, turn) = answered_with(&api);
    assert!(!system.contains(FULL_INDEX), "{system}");
    assert!(turn.contains(CHOSEN), "{turn}");
    assert!(turn.contains(&names[0]), "{turn}");
    for other in &names[1..] {
        assert!(!turn.contains(other.as_str()), "{other}: {turn}");
    }

    // Active, the decision failing: every skill, with the request.
    let roles = run(
        &api,
        &marc,
        vec![ScriptedStep::text("Je ne sais pas."), answer()],
    )
    .await;
    assert_eq!(roles, ["decision", "main"]);
    let (system, turn) = answered_with(&api);
    assert!(!system.contains(FULL_INDEX), "{system}");
    for name in &names {
        assert!(turn.contains(name.as_str()), "{name}: {turn}");
    }

    // Observed: asked and counted, the run lists them as before.
    set(&paul, choosing("observe")).await;
    let roles = run(&api, &marc, vec![first_useful(names.len()), answer()]).await;
    assert_eq!(roles, ["decision", "main"]);
    let (system, turn) = answered_with(&api);
    assert!(system.contains(FULL_INDEX), "{system}");
    assert!(!turn.contains(CHOSEN), "{turn}");

    // Off: nothing asked.
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
    assert_eq!(run(&api, &marc, vec![answer()]).await, ["main"]);
    assert!(answered_with(&api).0.contains(FULL_INDEX));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_decision_never_shows_skills_to_whom_may_not_read_them() {
    let api = TestApi::start_priced().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    set(&paul, choosing("active")).await;
    // The company's agents may not read skills.
    let refusal = json!({
        "description": "Politique de test",
        "permissions": ["skill:read"],
        "condition": { "op": "eq", "left": { "attr": "context.actorType" },
                       "right": { "value": "agent" } },
    });
    let created = paul.post("/admin/policies", refusal).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_string();
    let roles = run(&api, &marc, vec![ScriptedStep::text(ANSWER)]).await;
    // Nothing to choose from: nothing asked, nothing listed.
    assert_eq!(roles, ["main"]);
    let (system, turn) = answered_with(&api);
    assert!(!system.contains(FULL_INDEX), "{system}");
    assert!(!turn.contains(CHOSEN), "{turn}");
    for removed in [format!("/admin/policies/{id}"), DECISIONS.to_string()] {
        assert_eq!(paul.delete(&removed).await.status, StatusCode::OK);
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sub_agent_is_listed_the_skills_its_parent_was() {
    let api = TestApi::start_priced().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let names = skill_names(&api).await;
    set(&paul, choosing("active")).await;
    // The decision failing: every skill, to the parent and to its sub-agent.
    let delegated = ScriptedStep::calls(
        "",
        vec![call(
            "delegate_task",
            json!({ "profile": "research", "task": "Lister les interventions de la semaine" }),
        )],
    );
    let steps = vec![
        ScriptedStep::text("Je ne sais pas."),
        delegated,
        ScriptedStep::text("Trois interventions."),
        ScriptedStep::text(ANSWER),
    ];
    let roles = run(&api, &marc, steps).await;
    assert_eq!(roles, ["decision", "main", "main"]);
    let requests = api.model.as_ref().unwrap().requests();
    let child = requests
        .iter()
        .find(|request| request.system.contains("sous-agent"))
        .expect("the sub-agent's request");
    assert!(!child.system.contains(FULL_INDEX), "{}", child.system);
    let turn = format!("{:?}", child.messages.last().unwrap());
    assert!(turn.contains(CHOSEN), "{turn}");
    for name in &names {
        assert!(turn.contains(name.as_str()), "{name}: {turn}");
    }
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sub_agent_entrusted_after_a_validation_is_listed_them_too() {
    let api = TestApi::start_priced().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let names = skill_names(&api).await;
    set(&paul, choosing("active")).await;
    // The decision failing, every skill is listed; the run pauses on an
    // action to validate, and entrusts a task once it resumes, from the
    // checkpoint read back from the database.
    let delegated = ScriptedStep::calls(
        "",
        vec![call(
            "delegate_task",
            json!({ "profile": "research", "task": "Vérifier le planning de la semaine" }),
        )],
    );
    api.script(vec![
        ScriptedStep::text("Je ne sais pas."),
        propose("INT-2052", "16:00"),
        apply_proposal(""),
        delegated,
        ScriptedStep::text("Planning vérifié."),
        ScriptedStep::text(ANSWER),
    ]);
    let run = start_run(&marc).await;
    api.idle().await;
    let approvals = marc.get("/approvals").await.json();
    let approval = approvals["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["runId"] == json!(run))
        .expect("an action to validate")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let decided = marc
        .post(
            &format!("/approvals/{approval}/decision"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    api.idle().await;
    let requests = api.model.as_ref().unwrap().requests();
    let child = requests
        .iter()
        .find(|request| request.system.contains("sous-agent"))
        .expect("the sub-agent's request, once the run resumed");
    let turn = format!("{:?}", child.messages.last().unwrap());
    assert!(turn.contains(CHOSEN), "{turn}");
    for name in &names {
        assert!(turn.contains(name.as_str()), "{name}: {turn}");
    }
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
}
