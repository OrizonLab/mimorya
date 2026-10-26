//! The model a company chooses apart to review its answers (ADR-108): chosen
//! with its own key, shown, tested and removed by an administrator of its
//! settings; it judges the answers of the company's runs and evaluations in
//! place of their model. While it cannot be used, their model reviews; the
//! keys of the two never pass from one to the other.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::{ScriptedProvider, ScriptedStep, call};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::llm::{API_KEY, MASTER_KEY, OTHER_MASTER_KEY, REVIEW_MODEL};
use support::refusals::{entries_since, latest_entry};
use support::runs::start_run;
use support::skill_evaluations::{forget, launched, play, write_case};
use support::{Client, TestApi};
use uuid::Uuid;

const REVIEW: &str = "/admin/llm/verification";
const CRITERION: &str = "Nomme chaque client relancé.";
const NAME: &str = "relance-controlee";

fn review_choice() -> Value {
    json!({ "provider": "anthropic", "model": REVIEW_MODEL, "apiKey": API_KEY })
}

/// An active skill whose answers are reviewed on `CRITERION`; its id.
async fn reviewed_skill(paul: &Client<'_>) -> Uuid {
    let created = paul
        .post(
            "/admin/skills",
            json!({
                "name": NAME,
                "description": "Relancer les clients dont l’intervention attend une pièce.",
                "content": "# Relance\n1. Relancer chaque client.",
                "qualityCriteria": [CRITERION],
            }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let id: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    let path = format!("/admin/skills/{id}/activation");
    let activated = paul.put(&path, json!({ "version": 1 })).await;
    assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());
    id
}

async fn remove_skill(paul: &Client<'_>, id: Uuid) {
    paul.delete(&format!("/admin/skills/{id}/activation")).await;
    paul.delete(&format!("/admin/skills/{id}")).await;
}

fn verdict(pass: bool, issues: &[&str]) -> ScriptedStep {
    ScriptedStep::calls(
        "",
        vec![call(
            "submit_verdict",
            json!({ "pass": pass, "issues": issues }),
        )],
    )
}

/// The requests of `model` that asked for a verdict, and the others.
fn judged_by(model: &ScriptedProvider) -> (usize, usize) {
    let requests = model.requests();
    let judged = requests
        .iter()
        .filter(|r| r.tool_names() == ["submit_verdict"])
        .count();
    (judged, requests.len() - judged)
}

fn first_text(model: &ScriptedProvider) -> String {
    match &model.requests()[0].messages[0] {
        LlmMessage::User { content } => content.clone(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_review_model_judges_the_answers_until_it_is_removed() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let since = latest_entry(&api).await;
    let chosen = paul.put(REVIEW, review_choice()).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
    let shown = paul.get("/admin/llm").await.json();
    assert_eq!(shown["settings"], Value::Null, "the platform still answers");
    let review = &shown["verification"];
    assert_eq!(review["provider"], "anthropic");
    assert_eq!(review["model"], REVIEW_MODEL);
    assert_eq!(review["keyStored"], true);
    assert_eq!(review["usable"], true);
    let built = api.built.lock().unwrap().last().cloned().unwrap();
    assert_eq!(built.model.as_deref(), Some(REVIEW_MODEL));
    assert_eq!(built.api_key.as_deref(), Some(API_KEY));
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("verification_model.configured"), "{audit}");
    assert!(!audit.contains(API_KEY), "{audit}");

    // The platform's model answers and corrects; the review model judges.
    let skill = reviewed_skill(&paul).await;
    api.script(vec![
        ScriptedStep::calls("", vec![call("skill_read", json!({ "name": NAME }))]),
        ScriptedStep::text("Deux clients relancés."),
        ScriptedStep::text("Deux clients relancés : Dupont et Martin."),
    ]);
    api.review_model.push([
        verdict(false, &["Les clients ne sont pas nommés."]),
        verdict(true, &[]),
    ]);
    let run = start_run(&marc).await;
    api.idle().await;
    let detail = marc.get(&format!("/runs/{run}")).await.json();
    assert_eq!(
        detail["run"]["verification"],
        json!({ "passed": true, "issues": [], "judged": true }),
        "{detail}"
    );
    let platform = api.model.as_ref().unwrap();
    assert_eq!(judged_by(platform), (0, 3));
    assert_eq!(judged_by(&api.review_model), (2, 0));
    assert!(first_text(&api.review_model).contains(CRITERION));

    // Tested, then removed: the runs' model judges again.
    api.review_model.push([ScriptedStep::text("OK")]);
    let tested = paul.post(&format!("{REVIEW}/test"), json!({})).await.json();
    assert_eq!(tested["ok"], true, "{tested}");
    assert_eq!(paul.delete(REVIEW).await.status, StatusCode::OK);
    let removed = paul.delete(REVIEW).await;
    assert_eq!(removed.status, StatusCode::NOT_FOUND);
    assert!(
        removed
            .error_message()
            .contains("relues par le modèle de vos conversations"),
        "{}",
        removed.text()
    );
    assert_eq!(
        paul.get("/admin/llm").await.json()["verification"],
        Value::Null
    );
    let untested = paul.post(&format!("{REVIEW}/test"), json!({})).await;
    assert_eq!(untested.status, StatusCode::SERVICE_UNAVAILABLE);
    api.script(vec![
        ScriptedStep::calls("", vec![call("skill_read", json!({ "name": NAME }))]),
        ScriptedStep::text("Dupont et Martin relancés."),
        verdict(true, &[]),
    ]);
    start_run(&marc).await;
    api.idle().await;
    assert_eq!(judged_by(platform), (1, 5));
    assert_eq!(judged_by(&api.review_model), (2, 1));
    let audit = entries_since(&api, since).await;
    for action in ["verification_model.tested", "verification_model.reset"] {
        assert!(audit.contains(action), "{action}: {audit}");
    }
    remove_skill(&paul, skill).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_is_judged_by_the_review_model() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let paul = api.sign_in(user::PAUL).await;
    assert_eq!(
        paul.put(REVIEW, review_choice()).await.status,
        StatusCode::OK
    );
    let skill = reviewed_skill(&paul).await;
    let second = paul
        .post(
            &format!("/admin/skills/{skill}/versions"),
            json!({ "content": "# Relance\n1. Relancer." }),
        )
        .await;
    assert_eq!(second.status, StatusCode::CREATED, "{}", second.text());
    write_case(&paul, skill, "Relances", "Qui relancer aujourd’hui ?").await;
    let mut script = Vec::new();
    for answer in ["Dupont.", "Dupont et Martin."] {
        script.extend([
            ScriptedStep::calls("", vec![call("skill_read", json!({ "name": NAME }))]),
            ScriptedStep::text(answer),
        ]);
    }
    api.script(script);
    api.review_model
        .push([verdict(true, &[]), verdict(true, &[])]);
    // Version 2, then the active one beside it.
    launched(&paul, skill, 2).await;
    play(&api).await;

    assert_eq!(judged_by(api.model.as_ref().unwrap()), (0, 4));
    assert_eq!(judged_by(&api.review_model), (2, 0));
    assert!(first_text(&api.review_model).contains("Cite chaque retard."));
    forget(&api, skill).await;
    paul.delete(REVIEW).await;
    remove_skill(&paul, skill).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_unusable_review_model_leaves_the_runs_model_reviewing() {
    let configured = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let chooser = configured.sign_in(user::PAUL).await;
    assert_eq!(
        chooser.put(REVIEW, review_choice()).await.status,
        StatusCode::OK
    );

    // Another master key: the stored key does not open.
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", OTHER_MASTER_KEY)]).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let shown = paul.get("/admin/llm").await.json();
    assert_eq!(shown["verification"]["usable"], false, "{shown}");
    assert!(
        shown["verification"]["problem"]
            .as_str()
            .unwrap()
            .contains("déchiffrée"),
        "{shown}"
    );
    assert_eq!(
        marc.get("/auth/me").await.json()["assistant"]["available"],
        true,
        "the conversations go on"
    );
    let tested = paul.post(&format!("{REVIEW}/test"), json!({})).await;
    assert_eq!(tested.status, StatusCode::SERVICE_UNAVAILABLE);
    let skill = reviewed_skill(&paul).await;
    api.script(vec![
        ScriptedStep::calls("", vec![call("skill_read", json!({ "name": NAME }))]),
        ScriptedStep::text("Dupont et Martin relancés."),
        verdict(true, &[]),
    ]);
    let run = start_run(&marc).await;
    api.idle().await;
    let detail = marc.get(&format!("/runs/{run}")).await.json();
    assert_eq!(detail["run"]["verification"]["judged"], true, "{detail}");
    assert_eq!(judged_by(api.model.as_ref().unwrap()), (1, 2));
    assert!(api.review_model.requests().is_empty());

    assert_eq!(paul.delete(REVIEW).await.status, StatusCode::OK);
    remove_skill(&paul, skill).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_two_keys_never_pass_from_one_model_to_the_other() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    // Marc does not manage the company's settings.
    assert_eq!(
        marc.put(REVIEW, review_choice()).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(marc.delete(REVIEW).await.status, StatusCode::FORBIDDEN);
    let conversations = json!({ "provider": "anthropic", "apiKey": API_KEY });
    assert_eq!(
        paul.put("/admin/llm", conversations).await.status,
        StatusCode::OK
    );
    // The conversations' key does not serve the review model.
    let keyless = paul
        .put(
            REVIEW,
            json!({ "provider": "anthropic", "model": REVIEW_MODEL }),
        )
        .await;
    assert_eq!(
        keyless.status,
        StatusCode::BAD_REQUEST,
        "{}",
        keyless.text()
    );
    assert!(keyless.error_message().contains("Saisissez la clé d’API"));
    assert_eq!(
        paul.get("/admin/llm").await.json()["verification"],
        Value::Null
    );

    // Each removed alone, its key with it.
    assert_eq!(
        paul.put(REVIEW, review_choice()).await.status,
        StatusCode::OK
    );
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    let shown = paul.get("/admin/llm").await.json();
    assert_eq!(shown["settings"], Value::Null);
    assert_eq!(shown["verification"]["keyStored"], true, "{shown}");
    let again = json!({ "provider": "anthropic", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", again).await.status, StatusCode::OK);
    assert_eq!(paul.delete(REVIEW).await.status, StatusCode::OK);
    let shown = paul.get("/admin/llm").await.json();
    assert_eq!(shown["verification"], Value::Null);
    assert_eq!(shown["settings"]["keyStored"], true, "{shown}");
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}
