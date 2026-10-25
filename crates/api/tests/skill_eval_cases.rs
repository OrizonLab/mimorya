//! The evaluation cases of a skill (ADR-103): written, listed with the
//! skill, rewritten and deleted by its administrators, each audited without
//! its request; what is sent checked first; at most ten per skill, each
//! named once in it; gone with their skill. A member is refused, another
//! company's case answers as absent and stays as it was. Who reaches the
//! skill's routes: `skill_access.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_core::skill_eval_cases::MAX_CASES_PER_SKILL;
use mimorya_db::seed::DEMO_SKILL_ID;
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::refusals::{entries_since, latest_entry, not_found_since, refusals_since};
use support::{Client, TestApi};
use uuid::Uuid;

const SKILL: &str = "/admin/skills";
const CASES: &str = "/admin/skill-cases";

fn case(name: &str) -> Value {
    json!({
        "name": name,
        "request": "\r\nQuels retards cette semaine ?\r\nPar technicien.\r\n",
        "criteria": [" Cite chaque intervention en retard. ", "", "Nomme son technicien."],
    })
}

async fn create(paul: &Client<'_>, skill: Uuid, body: Value) -> String {
    let created = paul.post(&format!("{SKILL}/{skill}/cases"), body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_owned()
}

async fn cases(paul: &Client<'_>) -> Vec<Value> {
    let detail = paul.get(&format!("{SKILL}/{DEMO_SKILL_ID}")).await.json();
    detail["cases"].as_array().unwrap().clone()
}

/// The cases a company holds for skill `skill`.
async fn stored(api: &TestApi, tenant_id: Uuid, skill: Uuid) -> i64 {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query_scalar("select count(*) from skill_eval_cases where skill_id = $1")
        .bind(skill)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn cases_are_written_listed_rewritten_and_deleted() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;

    let first = create(&paul, DEMO_SKILL_ID, case("Retards de la semaine")).await;
    let second = create(&paul, DEMO_SKILL_ID, case("Retards du mois")).await;
    let listed = cases(&paul).await;
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0]["id"], first.as_str());
    assert_eq!(listed[0]["name"], "Retards de la semaine");
    assert_eq!(
        listed[0]["request"],
        "Quels retards cette semaine ?\nPar technicien."
    );
    assert_eq!(
        listed[0]["criteria"],
        json!([
            "Cite chaque intervention en retard.",
            "Nomme son technicien."
        ])
    );
    assert_eq!(listed[0]["createdBy"], "Paul Renaud");
    assert_eq!(listed[1]["id"], second.as_str());

    // Named once in the skill.
    let twice = paul
        .post(
            &format!("{SKILL}/{DEMO_SKILL_ID}/cases"),
            case("Retards du mois"),
        )
        .await;
    assert_eq!(twice.status, StatusCode::CONFLICT);
    assert!(twice.error_message().contains("porte déjà ce nom"));
    let renamed = paul
        .put(&format!("{CASES}/{first}"), case("Retards du mois"))
        .await;
    assert_eq!(renamed.status, StatusCode::CONFLICT);

    let rewritten = paul
        .put(
            &format!("{CASES}/{first}"),
            json!({
                "name": "Retards en cours",
                "request": "Quelles interventions sont en retard ?",
                "criteria": ["Classe les retards par ancienneté."],
            }),
        )
        .await;
    assert_eq!(rewritten.status, StatusCode::OK, "{}", rewritten.text());
    // Its own name, unchanged, is no conflict.
    let again = paul
        .put(&format!("{CASES}/{second}"), case("Retards du mois"))
        .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    let listed = cases(&paul).await;
    assert_eq!(listed[0]["name"], "Retards en cours");
    assert_eq!(
        listed[0]["request"],
        "Quelles interventions sont en retard ?"
    );
    assert_eq!(
        listed[0]["criteria"],
        json!(["Classe les retards par ancienneté."])
    );

    let deleted = paul.delete(&format!("{CASES}/{second}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
    assert_eq!(cases(&paul).await.len(), 1);
    let gone = paul.delete(&format!("{CASES}/{second}")).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    assert!(
        gone.error_message()
            .contains("Cas d’évaluation introuvable")
    );

    // Audited, the request never.
    let audit = entries_since(&api, since).await;
    for action in [
        "skill.eval_case_created",
        "skill.eval_case_updated",
        "skill.eval_case_deleted",
    ] {
        assert!(audit.contains(action), "{action}: {audit}");
    }
    assert!(
        audit.contains("\"previousCase\":\"Retards de la semaine\""),
        "{audit}"
    );
    assert!(!audit.contains("Quels retards"), "{audit}");
    assert!(!audit.contains("Quelles interventions"), "{audit}");

    paul.delete(&format!("{CASES}/{first}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_is_sent_is_checked_first() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let url = format!("{SKILL}/{DEMO_SKILL_ID}/cases");
    let long = "é".repeat(301);
    for (body, says) in [
        (
            json!({ "name": "Vide", "request": " \r\n ", "criteria": ["Un critère."] }),
            "La demande du cas est vide",
        ),
        (
            json!({ "name": "Sans critère", "request": "Demande", "criteria": [" ", ""] }),
            "critères de réussite",
        ),
        (
            json!({ "name": "Deux lignes", "request": "Demande", "criteria": ["Un\ndeux"] }),
            "Critère",
        ),
        (
            json!({ "name": "Long", "request": "Demande", "criteria": [long] }),
            "300 caractères",
        ),
        (
            json!({ "name": "Caché\u{200B}", "request": "Demande", "criteria": ["Critère."] }),
            "Nom du cas",
        ),
        (
            json!({ "name": "Caché", "request": "a\u{202E}b", "criteria": ["Critère."] }),
            "texte visible",
        ),
    ] {
        let refused = paul.post(&url, body).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{says}");
        assert!(
            refused.error_message().contains(says),
            "{says}: {}",
            refused.text()
        );
    }
    // Nine criteria, or a name of one letter: refused before the rules.
    let nine = vec!["Critère."; 9];
    for body in [
        json!({ "name": "Neuf", "request": "Demande", "criteria": nine }),
        json!({ "name": "N", "request": "Demande", "criteria": ["Critère."] }),
    ] {
        assert_eq!(paul.post(&url, body).await.status, StatusCode::BAD_REQUEST);
    }
    assert_eq!(stored(&api, tenant::ID, DEMO_SKILL_ID).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn at_most_ten_cases_per_skill_which_take_them_along() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let created = paul
        .post(
            SKILL,
            json!({
                "name": "relance-evaluee",
                "description": "Relancer un client dont l’intervention attend une pièce.",
                "content": "1. Rechercher l’intervention.",
            }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let skill: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();

    for n in 1..=MAX_CASES_PER_SKILL {
        create(&paul, skill, case(&format!("Cas {n}"))).await;
    }
    let eleventh = paul
        .post(&format!("{SKILL}/{skill}/cases"), case("Cas 11"))
        .await;
    assert_eq!(eleventh.status, StatusCode::BAD_REQUEST);
    assert!(eleventh.error_message().contains("Au plus 10 cas"));
    // Another skill's count is its own.
    let other = create(&paul, DEMO_SKILL_ID, case("Cas 11")).await;
    paul.delete(&format!("{CASES}/{other}")).await;

    let deleted = paul.delete(&format!("{SKILL}/{skill}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
    assert_eq!(stored(&api, tenant::ID, skill).await, 0);
}

/// A case of the other company's skill.
async fn foreign_case(api: &TestApi) -> (Uuid, Uuid) {
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let skill: Uuid = sqlx::query_scalar(
        "insert into skills (tenant_id, name, description) \
         values ($1, 'procedure-ailleurs', 'Une procédure d’une autre entreprise') returning id",
    )
    .bind(OTHER_TENANT_ID)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let case: Uuid = sqlx::query_scalar(
        "insert into skill_eval_cases (tenant_id, skill_id, name, request, criteria) \
         values ($1, $2, 'Ailleurs', 'Une demande d’ailleurs', array['Un critère.']) \
         returning id",
    )
    .bind(OTHER_TENANT_ID)
    .bind(skill)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (skill, case)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_member_is_refused_and_another_company_s_case_answers_as_absent() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let mine = create(&paul, DEMO_SKILL_ID, case("Retards de la semaine")).await;
    let since = latest_entry(&api).await;

    let rewrite = marc
        .put(&format!("{CASES}/{mine}"), case("Autre nom"))
        .await;
    assert_eq!(rewrite.status, StatusCode::FORBIDDEN);
    let delete = marc.delete(&format!("{CASES}/{mine}")).await;
    assert_eq!(delete.status, StatusCode::FORBIDDEN);
    assert_eq!(refusals_since(&api, since).await.len(), 2);
    assert_eq!(cases(&paul).await[0]["name"], "Retards de la semaine");

    let (skill, foreign) = foreign_case(&api).await;
    let since = latest_entry(&api).await;
    for id in [foreign, Uuid::new_v4()] {
        let rewrite = paul.put(&format!("{CASES}/{id}"), case("Autre nom")).await;
        assert_eq!(rewrite.status, StatusCode::NOT_FOUND);
        assert!(
            rewrite
                .error_message()
                .contains("Cas d’évaluation introuvable")
        );
        let delete = paul.delete(&format!("{CASES}/{id}")).await;
        assert_eq!(delete.status, StatusCode::NOT_FOUND);
    }
    let absent = not_found_since(&api, since).await;
    assert_eq!(absent.len(), 4);
    for (_, entry) in &absent[..2] {
        assert_eq!(
            entry.resource,
            Some(("skill_eval_case".into(), foreign.to_string()))
        );
    }

    // Untouched.
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let name: String = sqlx::query_scalar("select name from skill_eval_cases where id = $1")
        .bind(foreign)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(name, "Ailleurs");
    sqlx::query("delete from skills where id = $1")
        .bind(skill)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    paul.delete(&format!("{CASES}/{mine}")).await;
}
