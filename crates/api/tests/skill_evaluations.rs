//! A version of a skill tried on its cases (ADR-104): each case played for
//! the version evaluated and, beside it, for the active one, the agent
//! reading only, `skill_read` serving the version played, a model judging
//! each answer on the case's criteria; a case the active version passed and
//! the evaluated one did not is a regression, which making the version
//! active takes a confirmation against. Nothing of it is a run; what it
//! costs counts for whom it ran for. Their answers are read by them alone.
//! What is checked first, and how an evaluation ends early:
//! `skill_evaluation_rules.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use support::accounts::holder_of;
use support::refusals::{entries_since, latest_entry};
use support::skill_evaluations::{
    ISSUE, SECOND_VERSION, count, detail, evaluations, forget, launched, new_version, play, played,
    write_case,
};
use support::skill_proposals::{SKILL, SKILLS};
use support::{Client, TestApi};
use uuid::Uuid;

const LATE: &str = "Quels retards cette semaine ?";
const FOLLOW_UPS: &str = "Quels suivis faut-il planifier ?";

async fn activate(
    paul: &Client<'_>,
    version: i32,
    confirmed: Option<bool>,
) -> support::TestResponse {
    let mut body = json!({ "version": version });
    if let Some(confirmed) = confirmed {
        body["confirmed"] = json!(confirmed);
    }
    paul.put(&format!("{SKILLS}/{DEMO_SKILL_ID}/activation"), body)
        .await
}

fn user_turn(messages: &[LlmMessage]) -> String {
    match messages.first() {
        Some(LlmMessage::User { content }) => content.clone(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_version_is_played_on_each_case_beside_the_active_one() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let version = new_version(&paul).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;
    write_case(&paul, DEMO_SKILL_ID, "Suivis", FOLLOW_UPS).await;
    let mut script = played(true, "Trois retards cette semaine.", true);
    script.extend(played(true, "Trois retards, par technicien.", true));
    script.extend(played(false, "Je ne sais pas.", false));
    script.extend(played(true, "Deux suivis à planifier.", true));
    api.script(script);
    let (runs, usage) = (
        count(&api, "agent_runs").await,
        count(&api, "usage_records").await,
    );
    let since = latest_entry(&api).await;

    let id = launched(&paul, DEMO_SKILL_ID, version).await;
    let queued = evaluations(&paul, DEMO_SKILL_ID).await;
    assert_eq!(queued[0]["status"], "queued");
    assert_eq!(queued[0]["played"], 0);
    play(&api).await;

    let listed = evaluations(&paul, DEMO_SKILL_ID).await;
    let summary = &listed[0];
    assert_eq!(summary["id"], id.as_str());
    assert_eq!(summary["status"], "completed", "{summary}");
    assert_eq!(summary["version"], version);
    assert_eq!(summary["referenceVersion"], 1);
    assert_eq!(summary["cases"], 2);
    assert_eq!(summary["played"], 2);
    assert_eq!(summary["passed"], 1);
    assert_eq!(summary["referencePassed"], 2);
    assert_eq!(summary["regressions"], 1);
    assert_eq!(summary["launchedBy"], "Paul Renaud");

    let read = detail(&paul, &id).await.json();
    assert_eq!(read["detailsShown"], true);
    let (late, follow_ups) = (&read["cases"][0], &read["cases"][1]);
    assert_eq!(late["name"], "Retards");
    assert_eq!(late["result"]["outcome"], "passed");
    assert_eq!(late["result"]["skillLoaded"], true);
    assert_eq!(late["result"]["answer"], "Trois retards cette semaine.");
    assert_eq!(late["result"]["tools"][0]["name"], "skill_read");
    assert_eq!(late["regression"], false);
    assert_eq!(follow_ups["result"]["outcome"], "failed");
    assert_eq!(follow_ups["result"]["skillLoaded"], false);
    assert_eq!(follow_ups["result"]["issues"], json!([ISSUE]));
    assert_eq!(follow_ups["reference"]["outcome"], "passed");
    assert_eq!(follow_ups["regression"], true);

    // What the agent was given: the request, told it may only read, the
    // tools that read without validation, the version played.
    let requests = api.model.as_ref().unwrap().requests();
    let first = &requests[0];
    assert!(first.system.contains(SKILL));
    let turn = user_turn(&first.messages);
    assert!(turn.starts_with(LATE), "{turn}");
    assert!(turn.contains("ne sont pas disponibles"), "{turn}");
    let tools = first.tool_names();
    assert!(tools.contains(&"skill_read".to_string()), "{tools:?}");
    for withheld in [
        "planning_propose",
        "planning_apply",
        "memory_remember",
        "skill_propose",
        "delegate_task",
    ] {
        assert!(
            !tools.contains(&withheld.to_string()),
            "{withheld}: {tools:?}"
        );
    }
    let loaded = |request: usize| format!("{:?}", requests[request].messages.last());
    assert!(loaded(1).contains(SECOND_VERSION));
    // The judge (2), then the active version (3, 4).
    assert!(!loaded(4).contains(SECOND_VERSION));
    assert!(loaded(4).contains("Synthèse hebdomadaire"));

    // No run; what it cost counts for Paul.
    assert_eq!(count(&api, "agent_runs").await, runs);
    assert_eq!(count(&api, "usage_records").await, usage + 11);
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("skill.evaluation_requested"), "{audit}");
    assert!(audit.contains("skill.evaluated"), "{audit}");
    assert!(audit.contains("\"regressions\":1"), "{audit}");
    assert!(!audit.contains("Trois retards"), "{audit}");

    // Against its regression, only once confirmed.
    let refused = activate(&paul, version, None).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    let message = refused.error_message();
    assert!(
        message.contains("1 cas réussi par la version active ne l’est plus"),
        "{message}"
    );
    assert_eq!(
        activate(&paul, version, Some(true)).await.status,
        StatusCode::OK
    );
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("\"confirmed\":true"), "{audit}");
    // The way back: version 1 has no evaluation of its own.
    assert_eq!(activate(&paul, 1, None).await.status, StatusCode::OK);
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn whom_the_evaluation_ran_for_alone_reads_its_answers() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let version = new_version(&paul).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;
    let mut script = played(true, "Trois retards cette semaine.", false);
    script.extend(played(true, "Trois retards, par technicien.", true));
    api.script(script);
    let id = launched(&paul, DEMO_SKILL_ID, version).await;
    play(&api).await;

    let other = holder_of(&api, &paul, "lectrice.evaluations", &["skill:manage"]).await;
    let listed = evaluations(&other, DEMO_SKILL_ID).await;
    assert_eq!(listed[0]["regressions"], 1);
    let read = detail(&other, &id).await.json();
    assert_eq!(read["detailsShown"], false);
    let result = &read["cases"][0]["result"];
    assert_eq!(result["outcome"], "failed");
    for hidden in ["answer", "issues", "tools"] {
        assert_eq!(result[hidden], Value::Null, "{hidden}");
        assert_eq!(
            read["cases"][0]["reference"][hidden],
            Value::Null,
            "{hidden}"
        );
    }
    let mine = detail(&paul, &id).await.json();
    assert_eq!(mine["cases"][0]["result"]["issues"], json!([ISSUE]));
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_skill_not_active_yet_is_in_the_index_while_its_version_is_played() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let description = "Relancer un client dont l’intervention attend une pièce.";
    let created = paul
        .post(
            SKILLS,
            json!({ "name": "relance-evaluee", "description": description, "content": "1. Relancer." }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let skill: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    write_case(&paul, skill, "Relance", "Qui relancer aujourd’hui ?").await;
    api.script(played(false, "Personne aujourd’hui.", true));
    launched(&paul, skill, 1).await;
    play(&api).await;

    let listed = evaluations(&paul, skill).await;
    assert_eq!(listed[0]["status"], "completed", "{}", listed[0]);
    assert_eq!(listed[0]["referenceVersion"], Value::Null);
    assert_eq!(listed[0]["referencePassed"], Value::Null);
    assert_eq!(listed[0]["passed"], 1);
    let requests = api.model.as_ref().unwrap().requests();
    assert!(requests[0].system.contains("relance-evaluee"));
    assert!(requests[0].system.contains(description));
    forget(&api, skill).await;
    paul.delete(&format!("{SKILLS}/{skill}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_answer_the_judge_gives_no_verdict_on_is_an_error_kept_with_its_answer() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", LATE).await;
    write_case(&paul, DEMO_SKILL_ID, "Suivis", FOLLOW_UPS).await;
    api.script(vec![
        ScriptedStep::text("Trois retards cette semaine."),
        ScriptedStep::text("Je ne sais pas juger."),
        ScriptedStep::text("Deux suivis à planifier."),
        ScriptedStep::fails("Le modèle est surchargé."),
    ]);
    let usage = count(&api, "usage_records").await;
    let id = launched(&paul, DEMO_SKILL_ID, 1).await;
    play(&api).await;

    let read = detail(&paul, &id).await.json();
    assert_eq!(read["evaluation"]["status"], "completed");
    let (unjudged, failed) = (&read["cases"][0]["result"], &read["cases"][1]["result"]);
    assert_eq!(unjudged["outcome"], "error");
    assert_eq!(unjudged["answer"], "Trois retards cette semaine.");
    let error = |result: &Value| result["error"].as_str().unwrap().to_owned();
    assert!(error(unjudged).contains("pas rendu de verdict"));
    // The judge failing: its answer kept, what the agent spent counted.
    assert_eq!(failed["outcome"], "error");
    assert_eq!(failed["answer"], "Deux suivis à planifier.");
    assert_eq!(error(failed), "Le contrôle de la réponse a échoué.");
    assert_eq!(read["evaluation"]["passed"], 0);
    assert_eq!(count(&api, "usage_records").await, usage + 3);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let tokens: i64 =
        sqlx::query_scalar("select input_tokens from skill_evaluations where id = $1")
            .bind(id.parse::<Uuid>().unwrap())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(tokens, 300, "the agent twice, the judge once");
    forget(&api, DEMO_SKILL_ID).await;
}
