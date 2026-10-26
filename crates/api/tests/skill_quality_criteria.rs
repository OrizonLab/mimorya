//! Quality criteria of a skill's version (ADR-106): written with the
//! version by an administrator of the skills, trimmed, blank ones left out,
//! eight at most, each one visible line; a proposal of the agent's carries
//! none. A run that loads the active version has its answer reviewed by a
//! model on them, and corrected when it falls short; an evaluation's judge
//! reads them beside the case's criteria.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::refusals::{entries_since, latest_entry};
use support::runs::start_run;
use support::skill_evaluations::{forget, launched, play, played, write_case};
use support::{Client, TestApi};
use uuid::Uuid;

const SKILLS: &str = "/admin/skills";
const NAME: &str = "relance-datee";
const CRITERION: &str = "Nomme chaque client relancé.";

/// A skill whose first version holds `criteria`; its id.
async fn skill_with(paul: &Client<'_>, criteria: Value) -> Uuid {
    let created = paul
        .post(
            SKILLS,
            json!({
                "name": NAME,
                "description": "Relancer les clients dont l’intervention attend une pièce.",
                "content": "# Relance\n1. Relancer chaque client.",
                "qualityCriteria": criteria,
            }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().parse().unwrap()
}

async fn versions(paul: &Client<'_>, id: Uuid) -> Vec<Value> {
    let detail = paul.get(&format!("{SKILLS}/{id}")).await.json();
    detail["versions"].as_array().unwrap().clone()
}

fn user_text(message: &LlmMessage) -> String {
    match message {
        LlmMessage::User { content } => content.clone(),
        other => format!("{other:?}"),
    }
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn quality_criteria_are_written_with_a_version_and_checked() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = skill_with(
        &paul,
        json!([" Nomme chaque client relancé. ", "", "  ", "Date."]),
    )
    .await;
    let first = &versions(&paul, id).await[0];
    assert_eq!(
        first["qualityCriteria"],
        json!(["Nomme chaque client relancé.", "Date."])
    );

    let since = latest_entry(&api).await;
    let path = format!("{SKILLS}/{id}/versions");
    let nine: Vec<String> = (1..=9).map(|i| format!("Critère {i}.")).collect();
    for (criteria, says) in [
        (json!(nine), "8 critères de qualité au plus"),
        (
            json!(["Deux\u{200B}mots"]),
            "Critère de qualité : une seule ligne",
        ),
        (json!(["é".repeat(301)]), "300 caractères au plus"),
    ] {
        let refused = paul
            .post(
                &path,
                json!({ "content": "1. Autre.", "qualityCriteria": criteria }),
            )
            .await;
        assert!(refused.status.is_client_error(), "{}", refused.text());
        assert!(refused.error_message().contains(says), "{}", refused.text());
    }
    // Without any: none.
    let added = paul.post(&path, json!({ "content": "1. Autre." })).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    let listed = versions(&paul, id).await;
    assert_eq!(listed.len(), 2, "the refused ones were not written");
    assert_eq!(listed[0]["qualityCriteria"], json!([]));
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("\"qualityCriteria\":0"), "{audit}");

    paul.delete(&format!("{SKILLS}/{id}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_answer_that_followed_the_skill_is_reviewed_on_its_criteria() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let id = skill_with(&paul, json!([CRITERION])).await;
    let activated = paul
        .put(
            &format!("{SKILLS}/{id}/activation"),
            json!({ "version": 1 }),
        )
        .await;
    assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());

    api.script(vec![
        ScriptedStep::calls("", vec![call("skill_read", json!({ "name": NAME }))]),
        ScriptedStep::text("Deux clients relancés."),
        verdict(false, &["Les clients ne sont pas nommés."]),
        ScriptedStep::text("Deux clients relancés : Dupont et Martin."),
        verdict(true, &[]),
    ]);
    let run = start_run(&marc).await;
    api.idle().await;

    let requests = api.model.as_ref().unwrap().requests();
    let judged = user_text(&requests[2].messages[0]);
    assert!(judged.contains(&format!("1. {CRITERION}")), "{judged}");
    let corrected = format!("{:?}", requests[3].messages.last());
    assert!(
        corrected.contains("Les clients ne sont pas nommés."),
        "{corrected}"
    );
    let detail = marc.get(&format!("/runs/{run}")).await.json();
    assert_eq!(detail["run"]["status"], "completed", "{detail}");
    assert_eq!(
        detail["run"]["verification"],
        json!({ "passed": true, "issues": [], "judged": true })
    );

    paul.delete(&format!("{SKILLS}/{id}/activation")).await;
    paul.delete(&format!("{SKILLS}/{id}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_judges_the_answer_on_the_versions_criteria_too() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = skill_with(&paul, json!([CRITERION])).await;
    write_case(&paul, id, "Relances", "Qui relancer aujourd’hui ?").await;
    // The agent loads the version evaluated, answers, and the judge reads.
    let mut script = vec![ScriptedStep::calls(
        "",
        vec![call("skill_read", json!({ "name": NAME }))],
    )];
    script.extend(played(false, "Dupont et Martin.", true));
    api.script(script);
    launched(&paul, id, 1).await;
    play(&api).await;

    let requests = api.model.as_ref().unwrap().requests();
    let judged = user_text(&requests.last().unwrap().messages[0]);
    assert!(judged.contains("1. Cite chaque retard."), "{judged}");
    assert!(judged.contains(&format!("2. {CRITERION}")), "{judged}");
    forget(&api, id).await;
    paul.delete(&format!("{SKILLS}/{id}")).await;
}
