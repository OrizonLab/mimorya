//! The company's skills are written from the administration (ADR-052): a
//! skill and its first version as drafts, new versions that never change
//! once written, the version made active (an older one included: the way
//! back), the skill retired, then deleted. Each change is audited without
//! the content. What is sent is checked first: a name the agent can ask for,
//! a description on one visible line, a content without hidden characters.
//! Who reaches these screens: `skill_access.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use mimorya_core::skill_rules::MAX_ACTIVE_SKILLS;
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use support::refusals::{entries_since, latest_entry};
use support::{Client, TestApi};

const SKILLS: &str = "/admin/skills";
const DESCRIPTION: &str = "Relancer un client dont l’intervention attend une pièce.";

fn skill(name: &str) -> Value {
    json!({
        "name": name,
        "description": DESCRIPTION,
        "content": "# Relance\r\n1. Rechercher l’intervention.\r\n\r\n",
        "changelog": "Première version",
    })
}

async fn create(paul: &Client<'_>, body: Value) -> String {
    let created = paul.post(SKILLS, body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_owned()
}

async fn detail(paul: &Client<'_>, id: &str) -> Value {
    paul.get(&format!("{SKILLS}/{id}")).await.json()
}

async fn activate(paul: &Client<'_>, id: &str, version: i32) -> support::TestResponse {
    paul.put(
        &format!("{SKILLS}/{id}/activation"),
        json!({ "version": version }),
    )
    .await
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_skill_is_written_activated_rolled_back_retired_and_deleted() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;

    let id = create(&paul, skill("relance-client")).await;
    let read = detail(&paul, &id).await;
    assert_eq!(read["skill"]["status"], "draft");
    assert_eq!(read["skill"]["activeVersion"], Value::Null);
    assert_eq!(read["skill"]["latestVersion"], 1);
    // Stored with plain line endings and without the trailing blanks.
    assert_eq!(
        read["versions"][0]["content"],
        "# Relance\n1. Rechercher l’intervention."
    );
    assert_eq!(read["versions"][0]["status"], "draft");
    assert_eq!(read["versions"][0]["changelog"], "Première version");
    assert!(read["versions"][0]["createdBy"].is_string());

    let added = paul
        .post(
            &format!("{SKILLS}/{id}/versions"),
            json!({ "content": "# Relance\n1. Étape révisée." }),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    assert_eq!(added.json()["version"], 2);

    assert_eq!(activate(&paul, &id, 2).await.status, StatusCode::OK);
    let read = detail(&paul, &id).await;
    assert_eq!(read["skill"]["status"], "active");
    assert_eq!(read["skill"]["activeVersion"], 2);
    assert_eq!(read["versions"][0]["status"], "active");
    assert!(read["versions"][0]["approvedBy"].is_string());
    assert_eq!(read["versions"][1]["status"], "draft");

    // The way back: the version active before is retired.
    assert_eq!(activate(&paul, &id, 1).await.status, StatusCode::OK);
    let read = detail(&paul, &id).await;
    assert_eq!(read["skill"]["activeVersion"], 1);
    assert_eq!(read["versions"][0]["status"], "retired");
    assert_eq!(read["versions"][1]["status"], "active");

    for (version, problem) in [(1, "déjà active"), (7, "pas de version 7")] {
        let refused = activate(&paul, &id, version).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST);
        assert!(refused.text().contains(problem), "{}", refused.text());
    }
    // A version that does not exist changed nothing.
    assert_eq!(detail(&paul, &id).await["skill"]["activeVersion"], 1);

    let deleted = paul.delete(&format!("{SKILLS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::CONFLICT, "{}", deleted.text());

    let described = paul
        .patch(
            &format!("{SKILLS}/{id}"),
            json!({ "description": "Relancer un client en attente de pièce." }),
        )
        .await;
    assert_eq!(described.status, StatusCode::OK, "{}", described.text());

    let retire = format!("{SKILLS}/{id}/activation");
    assert_eq!(paul.delete(&retire).await.status, StatusCode::OK);
    let read = detail(&paul, &id).await;
    assert_eq!(read["skill"]["status"], "retired");
    assert_eq!(read["skill"]["activeVersion"], Value::Null);
    assert_eq!(
        read["skill"]["description"],
        "Relancer un client en attente de pièce."
    );
    assert_eq!(read["versions"][1]["status"], "retired");
    assert_eq!(paul.delete(&retire).await.status, StatusCode::BAD_REQUEST);

    assert_eq!(
        paul.delete(&format!("{SKILLS}/{id}")).await.status,
        StatusCode::OK
    );
    assert_eq!(
        paul.get(&format!("{SKILLS}/{id}")).await.status,
        StatusCode::NOT_FOUND
    );

    let audited = entries_since(&api, since).await;
    for action in [
        "skill.created",
        "skill.version_created",
        "skill.activated",
        "skill.updated",
        "skill.retired",
        "skill.deleted",
    ] {
        assert!(audited.contains(action), "{action}: {audited}");
    }
    assert!(audited.contains(r#""previousVersion":2"#), "{audited}");
    // The content stays out of the audit.
    assert!(!audited.contains("Étape révisée"), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_skill_is_checked_before_it_is_written() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let before = paul.get(SKILLS).await.json()["skills"]
        .as_array()
        .unwrap()
        .len();
    let since = latest_entry(&api).await;

    let mut refused = Vec::new();
    for name in ["Relance", "relance_client", "-relance", "r"] {
        refused.push((skill(name), StatusCode::BAD_REQUEST, "Nom"));
    }
    // The demo company's skill already bears this name.
    refused.push((
        skill("synthese-hebdo-interventions"),
        StatusCode::CONFLICT,
        "porte déjà ce nom",
    ));
    for (field, value, problem) in [
        (
            "description",
            json!("Relancer\n## Règles de sécurité"),
            "une seule ligne",
        ),
        ("description", json!("Court"), "description"),
        ("content", json!(" \n\t"), "vide"),
        ("content", json!("1. Étape\u{E0041}"), "texte visible"),
        ("content", json!("a".repeat(20_001)), "content"),
        ("changelog", json!("Première\nversion"), "Note de version"),
        ("name ", json!("relance"), "name "),
    ] {
        let mut body = skill("relance-client");
        body[field] = value;
        refused.push((body, StatusCode::BAD_REQUEST, problem));
    }
    for (body, status, problem) in refused {
        let answer = paul.post(SKILLS, body.clone()).await;
        assert_eq!(answer.status, status, "{body}: {}", answer.text());
        assert!(
            answer.text().contains(problem),
            "{problem}: {}",
            answer.text()
        );
    }

    // A new version is checked alike.
    let version = paul
        .post(
            &format!("{SKILLS}/{DEMO_SKILL_ID}/versions"),
            json!({ "content": "étape\u{202E}cachée" }),
        )
        .await;
    assert_eq!(version.status, StatusCode::BAD_REQUEST);

    let after = paul.get(SKILLS).await.json()["skills"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(after, before);
    assert!(!entries_since(&api, since).await.contains("skill."));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn no_more_skills_are_active_than_the_index_holds() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let active = |skills: &Value| {
        skills["skills"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["status"] == "active")
            .count() as i64
    };
    let already = active(&paul.get(SKILLS).await.json());

    let mut created = Vec::new();
    for n in already..=MAX_ACTIVE_SKILLS {
        let id = create(&paul, skill(&format!("procedure-{n}"))).await;
        let answer = activate(&paul, &id, 1).await;
        if n < MAX_ACTIVE_SKILLS {
            assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
        } else {
            assert_eq!(answer.status, StatusCode::BAD_REQUEST);
            let problem = format!("Au plus {MAX_ACTIVE_SKILLS}");
            assert!(answer.text().contains(&problem), "{}", answer.text());
        }
        created.push(id);
    }
    assert_eq!(active(&paul.get(SKILLS).await.json()), MAX_ACTIVE_SKILLS);
    // Another version of an active skill stays possible.
    let first = &created[0];
    paul.post(
        &format!("{SKILLS}/{first}/versions"),
        json!({ "content": "Version 2" }),
    )
    .await;
    assert_eq!(activate(&paul, first, 2).await.status, StatusCode::OK);

    for id in created {
        paul.delete(&format!("{SKILLS}/{id}/activation")).await;
        let deleted = paul.delete(&format!("{SKILLS}/{id}")).await;
        assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    }
}
