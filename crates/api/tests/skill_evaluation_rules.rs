//! What holds an evaluation of a skill back, or ends it early (ADR-104):
//! asked for by an administrator of the company's skills who may use the
//! agent, the agent reading the skills for them, the company having a
//! model, for a version the skill has and its cases, one at a time in the
//! company; another company's skill or evaluation answers as absent. It
//! ends, failed, once whom it runs for may no longer administer the skills
//! or use the agent (their account disabled, their role narrowed). The last
//! five of a skill are kept, ninety days at most. How one is played:
//! `skill_evaluations.rs`; the passes that play them, one per company:
//! `skill_evaluation_passes.rs`.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_core::demo::{tenant, user};
use mimorya_core::skill_evaluations::EVALUATION_KEPT_DAYS;
use mimorya_db::repositories::skill_evaluations::purge_finished_evaluations;
use mimorya_db::seed::DEMO_SKILL_ID;
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::json;
use support::accounts::holder_of;
use support::queue::API;
use support::skill_evaluations::{
    count, detail, evaluations, forget, launch, launched, new_version, play, status_of,
    stored_evaluation, write_case,
};
use support::{Client, TestApi};
use uuid::Uuid;

async fn refused(client: &Client<'_>, skill: Uuid, version: i32, status: StatusCode, says: &str) {
    let answer = launch(client, skill, version).await;
    assert_eq!(answer.status, status, "{says}: {}", answer.text());
    assert!(
        answer.error_message().contains(says),
        "{says}: {}",
        answer.text()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_is_checked_before_an_evaluation_is_queued() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let bad = StatusCode::BAD_REQUEST;
    refused(&paul, DEMO_SKILL_ID, 1, bad, "au moins un cas").await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", "Quels retards ?").await;
    refused(&paul, DEMO_SKILL_ID, 99, bad, "pas de version 99").await;
    refused(
        &marc,
        DEMO_SKILL_ID,
        1,
        StatusCode::FORBIDDEN,
        "skill:manage",
    )
    .await;
    // The cases run for whom asks: they must be able to use the agent.
    let permissions = ["skill:manage", "agent:use", "skill:read"];
    let other = holder_of(&api, &paul, "sans.assistant", &permissions).await;
    let me = other.get("/auth/me").await.json();
    let only_them = json!({
        "op": "eq",
        "left": { "attr": "subject.userId" },
        "right": { "value": me["user"]["id"] },
    });
    let policy = paul
        .post(
            "/admin/policies",
            json!({ "description": "Sans assistant", "permissions": ["agent:use"], "condition": only_them }),
        )
        .await;
    assert_eq!(policy.status, StatusCode::CREATED, "{}", policy.text());
    let forbidden = StatusCode::FORBIDDEN;
    refused(&other, DEMO_SKILL_ID, 1, forbidden, "utiliser l’assistant").await;
    let policy = format!("/admin/policies/{}", policy.json()["id"].as_str().unwrap());
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    refused(
        &paul,
        Uuid::new_v4(),
        1,
        StatusCode::NOT_FOUND,
        "Compétence introuvable",
    )
    .await;

    // One at a time in the company; this process plays none.
    let id = launched(&paul, DEMO_SKILL_ID, 1).await;
    refused(
        &paul,
        DEMO_SKILL_ID,
        1,
        StatusCode::CONFLICT,
        "déjà en cours",
    )
    .await;
    assert_eq!(status_of(&api, &id).await.0, "queued");

    // Another company's evaluation answers as one that exists nowhere.
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let skill: Uuid = sqlx::query_scalar(
        "insert into skills (tenant_id, name, description) \
         values ($1, 'procedure-ailleurs', 'Une procédure d’une autre entreprise') returning id",
    )
    .bind(OTHER_TENANT_ID)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let foreign: Uuid = sqlx::query_scalar(
        "insert into skill_evaluations (tenant_id, skill_id, version, cases) \
         values ($1, $2, 1, '[]') returning id",
    )
    .bind(OTHER_TENANT_ID)
    .bind(skill)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let absent = detail(&paul, &foreign.to_string()).await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND);
    assert!(absent.error_message().contains("Évaluation introuvable"));
    let listed = paul
        .get(&format!("/admin/skills/{skill}/evaluations"))
        .await;
    assert_eq!(listed.status, StatusCode::NOT_FOUND);
    refused(
        &paul,
        skill,
        1,
        StatusCode::NOT_FOUND,
        "Compétence introuvable",
    )
    .await;

    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query("delete from skills where id = $1")
        .bind(skill)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_model_or_the_agent_reading_the_skills_nothing_is_queued() {
    let api = TestApi::without_llm().await;
    let paul = api.sign_in(user::PAUL).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", "Quels retards ?").await;
    let bad = StatusCode::BAD_REQUEST;
    refused(&paul, DEMO_SKILL_ID, 1, bad, "Aucun modèle").await;

    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let agents = json!({ "op": "eq", "left": { "attr": "context.actorType" }, "right": { "value": "agent" } });
    let policy = paul
        .post(
            "/admin/policies",
            json!({ "description": "Compétences fermées", "permissions": ["skill:read"], "condition": agents }),
        )
        .await;
    assert_eq!(policy.status, StatusCode::CREATED, "{}", policy.text());
    refused(&paul, DEMO_SKILL_ID, 1, bad, "ne lit pas les compétences").await;
    let policy = format!("/admin/policies/{}", policy.json()["id"].as_str().unwrap());
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    assert_eq!(count(&api, "skill_evaluations").await, 0);
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_ends_once_whom_it_runs_for_may_no_longer() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let permissions = ["skill:manage", "agent:use", "skill:read"];
    let other = holder_of(&api, &paul, "evaluatrice", &permissions).await;
    let version = new_version(&paul).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", "Quels retards ?").await;
    let id = launched(&other, DEMO_SKILL_ID, version).await;
    let me = other.get("/auth/me").await.json();
    let account = format!("/admin/users/{}", me["user"]["id"].as_str().unwrap());
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());

    play(&api).await;
    let (status, failure, results) = status_of(&api, &id).await;
    assert_eq!(status, "failed");
    assert!(failure.unwrap().contains("n’est plus actif"));
    assert_eq!(results, 0);
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_ends_once_whom_it_runs_for_no_longer_administers_the_skills() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let permissions = ["skill:manage", "agent:use", "skill:read"];
    let other = holder_of(&api, &paul, "ex.administratrice", &permissions).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", "Quels retards ?").await;
    let id = launched(&other, DEMO_SKILL_ID, 1).await;
    // Their role no longer lets them administer the skills; they may still
    // use the agent.
    let roles = paul.get("/admin/roles").await.json();
    let role = roles["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Rôle ex.administratrice")
        .unwrap();
    let narrowed = paul
        .patch(
            &format!("/admin/roles/{}", role["id"].as_str().unwrap()),
            json!({ "permissions": ["agent:use", "skill:read"] }),
        )
        .await;
    assert_eq!(narrowed.status, StatusCode::OK, "{}", narrowed.text());

    play(&api).await;
    let (status, failure, results) = status_of(&api, &id).await;
    assert_eq!(status, "failed");
    assert!(failure.unwrap().contains("n’administre plus"));
    assert_eq!(results, 0);
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn evaluations_ended_ninety_days_ago_are_deleted() {
    // A process that executes no runs: no maintenance pass of its own
    // purges them before the test does.
    let api = TestApi::start_with(API).await;
    let old = stored_evaluation(&api, "completed", 100).await;
    let recent = stored_evaluation(&api, "failed", 10).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update skill_evaluations set finished_at = created_at")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let pool = api.app.services.db.pool();
    let before = Utc::now() - chrono::Duration::days(EVALUATION_KEPT_DAYS);
    assert_eq!(purge_finished_evaluations(pool, before).await.unwrap(), 1);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let left: Vec<Uuid> = sqlx::query_scalar("select id from skill_evaluations")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(left, [recent]);
    assert!(!left.contains(&old));
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_last_five_evaluations_of_a_skill_are_kept() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let oldest = stored_evaluation(&api, "completed", 9).await;
    for days in 4..=7 {
        stored_evaluation(
            &api,
            if days % 2 == 0 { "completed" } else { "failed" },
            days,
        )
        .await;
    }
    write_case(&paul, DEMO_SKILL_ID, "Retards", "Quels retards ?").await;
    let newest = launched(&paul, DEMO_SKILL_ID, 1).await;

    let listed = evaluations(&paul, DEMO_SKILL_ID).await;
    assert_eq!(listed.len(), 5);
    assert_eq!(listed[0]["id"], newest.as_str());
    assert!(
        listed
            .iter()
            .all(|e| e["id"] != oldest.to_string().as_str())
    );
    assert_eq!(count(&api, "skill_evaluations").await, 5);
    forget(&api, DEMO_SKILL_ID).await;
}
