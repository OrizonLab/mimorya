//! Skills the agent proposes (ADR-100): a turn whose agent proposes one,
//! its validation, what the model reads of the result, the weekly report
//! put back as the seed left it, and the proposals made to wait.

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::skills::{NewSkillVersion, insert_skill_version};
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use uuid::Uuid;

use super::manager_approvals::listed;
use super::{Client, TestApi};

pub const SKILL: &str = "synthese-hebdo-interventions";
pub const SKILLS: &str = "/admin/skills";
pub const RATIONALE: &str = "Ajoute la relecture des interventions en retard.";

pub fn version_of_the_report() -> Value {
    json!({
        "name": SKILL,
        "content": "# Synthèse\r\n1. Lister les interventions.\r\n2. Relire les retards.\r\n\r\n",
        "rationale": RATIONALE,
    })
}

/// A turn of `lucas`'s (or another's) in which the agent proposes `input`,
/// then reads its result with `then`; the run, paused on a validation, and
/// the request.
pub async fn proposed(
    api: &TestApi,
    lucas: &Client<'_>,
    input: Value,
    then: ScriptedStep,
) -> (Uuid, String) {
    api.script(vec![
        ScriptedStep::calls("", vec![call("skill_propose", input)]),
        then,
    ]);
    let conversation = lucas.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let sent = lucas
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Propose d’améliorer la procédure de synthèse." }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    api.idle().await;
    let run = sent.json()["runId"].as_str().unwrap().to_string();
    let request = listed(lucas, &run)
        .await
        .unwrap_or_else(|| panic!("the request of {run}"));
    assert_eq!(request["toolName"], "skill_propose");
    (run.parse().unwrap(), request["id"].as_str().unwrap().into())
}

/// Lucas validates `approval`; the run then ends.
pub async fn validate(api: &TestApi, lucas: &Client<'_>, approval: &str) {
    let decided = lucas
        .post(
            &format!("/approvals/{approval}/decision"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    api.idle().await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0, "steps left");
}

/// The model reads a result that went through.
pub fn read_proposed() -> ScriptedStep {
    ScriptedStep::dynamic(|req| {
        let results = last_tool_results(req);
        assert!(!results[0].is_error, "{}", results[0].content);
        ScriptedReply::text("Proposition envoyée aux administrateurs.")
    })
}

/// The model reads a refusal saying `why`.
pub fn read_refused(why: &'static str) -> ScriptedStep {
    ScriptedStep::dynamic(move |req| {
        let results = last_tool_results(req);
        assert!(results[0].is_error, "{}", results[0].content);
        assert!(results[0].content.contains(why), "{}", results[0].content);
        ScriptedReply::text("La proposition est refusée.")
    })
}

/// The weekly report as the seed left it, for the tests after: its first
/// version alone, active.
pub async fn restore_the_report(api: &TestApi) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let first: Uuid =
        sqlx::query_scalar("select id from skill_versions where skill_id = $1 and version = 1")
            .bind(DEMO_SKILL_ID)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    for statement in [
        "update skills set status = 'active', active_version_id = $2 where id = $1",
        "update skill_versions set status = 'active' where skill_id = $1 and id = $2",
        "delete from skill_versions where skill_id = $1 and id <> $2",
    ] {
        sqlx::query(statement)
            .bind(DEMO_SKILL_ID)
            .bind(first)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
}

pub async fn detail(paul: &Client<'_>, id: &str) -> Value {
    let shown = paul.get(&format!("{SKILLS}/{id}")).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    shown.json()
}

/// The skills' audit entries written after `since`: action and metadata.
pub async fn audited(api: &TestApi, since: i64) -> Vec<(String, String, Option<Uuid>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, actor_type, on_behalf_of_user_id, metadata from audit_log \
         where id > $1 and action like 'skill.%' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// `count` proposals waiting on the weekly report, written for `author`
/// by `run`.
pub async fn waiting(api: &TestApi, count: i64, author: Uuid, run: Uuid) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    for _ in 0..count {
        let waiting = NewSkillVersion {
            tenant_id: tenant::ID,
            skill_id: DEMO_SKILL_ID,
            content: "1. Une proposition.",
            changelog: "En attente.",
            quality_criteria: None,
            created_by: author,
            proposed_by_run: Some(run),
            description: None,
        };
        insert_skill_version(&mut tx, waiting).await.unwrap();
    }
    tx.commit().await.unwrap();
}

/// The display name of `user_id`.
pub async fn name_of(api: &TestApi, user_id: Uuid) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar("select display_name from users where id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}
