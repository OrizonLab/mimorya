//! A proposal of the agent's (ADR-100) asks for the company's escalation
//! model (ADR-207) as the version active does, and as nothing else: the
//! agent does not choose the model a skill's runs go on with, its
//! administrators do.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use support::skill_proposals::{
    SKILLS, detail, proposed, read_proposed, restore_the_report, validate, version_of_the_report,
};
use support::{Client, TestApi};

/// The latest version of the weekly report.
async fn latest(paul: &Client<'_>) -> Value {
    let shown = detail(paul, &DEMO_SKILL_ID.to_string()).await;
    shown["versions"][0].clone()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_of_the_agent_asks_for_the_escalation_model_as_the_version_active_does() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    for asks in [false, true] {
        let written = paul
            .post(
                &format!("{SKILLS}/{DEMO_SKILL_ID}/versions"),
                json!({ "content": "# Synthèse\n1. Lister.", "modelEscalation": asks }),
            )
            .await;
        assert_eq!(written.status, StatusCode::CREATED, "{}", written.text());
        let version = written.json()["version"].clone();
        let activated = paul
            .put(
                &format!("{SKILLS}/{DEMO_SKILL_ID}/activation"),
                json!({ "version": version }),
            )
            .await;
        assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());

        let (_, approval) = proposed(&api, &lucas, version_of_the_report(), read_proposed()).await;
        validate(&api, &lucas, &approval).await;
        let proposal = latest(&paul).await;
        assert_eq!(proposal["proposed"], true);
        assert_eq!(proposal["modelEscalation"], asks, "{proposal}");
        restore_the_report(&api).await;
    }
}
