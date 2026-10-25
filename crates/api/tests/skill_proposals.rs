//! Skills the agent proposes (ADR-100): once the person validated its
//! text, a proposal is a candidate version among the skill's, or a new
//! skill's first, which runs do not read before an administrator makes it
//! active. An administrator turns one down, and may still make it active;
//! only a proposal awaiting a decision is turned down. An inactive skill's
//! name is answered as a name no skill bears. Each step is audited without
//! the procedure's text. Its bounds and its validation by a manager:
//! `skill_proposal_bounds.rs`; who reaches the administration's routes:
//! `skill_access.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, last_tool_results};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::skills::get_active_skill;
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use support::TestApi;
use support::refusals::latest_entry;
use support::skill_proposals::{
    RATIONALE, SKILL, SKILLS, audited, detail, name_of, proposed, read_proposed, read_refused,
    restore_the_report, validate, version_of_the_report,
};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_validated_proposal_waits_for_the_administrators_as_a_candidate() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let skill = DEMO_SKILL_ID.to_string();

    let (run, approval) = proposed(&api, &lucas, version_of_the_report(), read_proposed()).await;
    // Nothing reaches the administrators before Lucas validates it.
    assert_eq!(detail(&paul, &skill).await["skill"]["latestVersion"], 1);
    validate(&api, &lucas, &approval).await;

    let shown = detail(&paul, &skill).await;
    assert_eq!(shown["skill"]["activeVersion"], 1);
    assert_eq!(shown["skill"]["latestVersion"], 2);
    assert_eq!(shown["skill"]["proposals"], 1);
    let proposal = &shown["versions"][0];
    assert_eq!(proposal["version"], 2);
    assert_eq!(proposal["status"], "candidate");
    assert_eq!(proposal["proposed"], true);
    assert_eq!(proposal["changelog"], RATIONALE);
    assert_eq!(
        proposal["content"],
        "# Synthèse\n1. Lister les interventions.\n2. Relire les retards."
    );
    let lucas_name = name_of(&api, user::LUCAS).await;
    assert_eq!(proposal["createdBy"], lucas_name.as_str());
    assert_eq!(shown["versions"][1]["proposed"], false);

    // Runs still read the version made active.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let read = get_active_skill(&mut tx, SKILL).await.unwrap().unwrap();
    tx.commit().await.unwrap();
    assert_eq!(read.version, 1);

    // Audited for Lucas by his run's agent, without the procedure's text.
    let entries = audited(&api, since).await;
    let (action, actor, on_behalf_of, metadata) = &entries[0];
    assert_eq!(action, "skill.proposed");
    assert_eq!(actor, "agent");
    assert_eq!(*on_behalf_of, Some(user::LUCAS));
    assert_eq!(
        *metadata,
        json!({ "name": SKILL, "version": 2, "newSkill": false })
    );
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let (run_of, resource): (Option<Uuid>, Option<String>) = sqlx::query_as(
        "select run_id, resource_id from audit_log where id > $1 and action = 'skill.proposed'",
    )
    .bind(since)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(run_of, Some(run));
    assert_eq!(resource.as_deref(), Some(skill.as_str()));

    // Made active, it is what runs read; it says where it came from.
    let activated = paul
        .put(
            &format!("{SKILLS}/{skill}/activation"),
            json!({ "version": 2 }),
        )
        .await;
    assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());
    let shown = detail(&paul, &skill).await;
    assert_eq!(shown["skill"]["activeVersion"], 2);
    assert_eq!(shown["skill"]["proposals"], 0);
    assert_eq!(shown["versions"][0]["status"], "active");
    assert_eq!(shown["versions"][0]["proposed"], true);
    restore_the_report(&api).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_new_skill_proposed_is_a_draft_whose_proposal_is_turned_down() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let paul = api.sign_in(user::PAUL).await;
    let new_skill = json!({
        "name": "relance-piece",
        "description": "Relancer un fournisseur dont la pièce tarde.",
        "content": "1. Retrouver la commande.\n2. Relancer le fournisseur.",
        "rationale": "Procédure suivie avec succès aujourd’hui.",
    });
    let (_, approval) = proposed(&api, &lucas, new_skill, read_proposed()).await;
    validate(&api, &lucas, &approval).await;

    let listed = paul.get(SKILLS).await.json();
    let skill = listed["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "relance-piece")
        .expect("the skill proposed")
        .clone();
    assert_eq!(skill["status"], "draft");
    assert_eq!(
        skill["description"],
        "Relancer un fournisseur dont la pièce tarde."
    );
    assert_eq!(skill["activeVersion"], Value::Null);
    assert_eq!(skill["proposals"], 1);
    let id = skill["id"].as_str().unwrap();
    let version = &detail(&paul, id).await["versions"][0];
    assert_eq!(version["status"], "candidate");
    assert_eq!(
        version["proposedDescription"],
        "Relancer un fournisseur dont la pièce tarde."
    );

    let since = latest_entry(&api).await;
    let rejection = format!("{SKILLS}/{id}/rejection");
    let rejected = paul.post(&rejection, json!({ "version": 1 })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    let shown = detail(&paul, id).await;
    assert_eq!(shown["skill"]["proposals"], 0);
    assert_eq!(shown["versions"][0]["status"], "rejected");
    assert_eq!(shown["versions"][0]["proposed"], true);
    let entries = audited(&api, since).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, "skill.proposal_rejected");
    assert_eq!(entries[0].1, "user");
    assert_eq!(
        entries[0].3,
        json!({ "name": "relance-piece", "version": 1 })
    );

    // Only a proposal awaiting a decision is turned down.
    for version in [1, 2] {
        let again = paul.post(&rejection, json!({ "version": version })).await;
        assert_eq!(again.status, StatusCode::BAD_REQUEST, "{}", again.text());
        assert!(
            again
                .text()
                .contains("n’est pas une proposition en attente")
        );
    }
    let written = paul
        .post(
            &format!("{SKILLS}/{DEMO_SKILL_ID}/rejection"),
            json!({ "version": 1 }),
        )
        .await;
    assert_eq!(
        written.status,
        StatusCode::BAD_REQUEST,
        "{}",
        written.text()
    );

    // Turned down, a proposal may still be made active.
    let activation = format!("{SKILLS}/{id}/activation");
    let activated = paul.put(&activation, json!({ "version": 1 })).await;
    assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());
    let shown = detail(&paul, id).await;
    assert_eq!(shown["skill"]["status"], "active");
    assert_eq!(shown["versions"][0]["status"], "active");
    assert_eq!(paul.delete(&activation).await.status, StatusCode::OK);
    let deleted = paul.delete(&format!("{SKILLS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_inactive_skill_is_answered_as_a_name_no_skill_bears() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let paul = api.sign_in(user::PAUL).await;

    // A skill of the company, never active: Lucas cannot read it.
    let created = paul
        .post(
            SKILLS,
            json!({
                "name": "relance-client",
                "description": "Relancer un client dont l’intervention attend.",
                "content": "1. Rechercher l’intervention.",
            }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let draft = created.json()["id"].as_str().unwrap().to_string();

    // A version of it is refused as one of a name no skill bears…
    let version = json!({
        "name": "relance-client",
        "content": "1. Appeler le client.",
        "rationale": "Utile.",
    });
    let unknown = "Aucune compétence active ne s’appelle « relance-client »";
    let (_, approval) = proposed(&api, &lucas, version, read_refused(unknown)).await;
    validate(&api, &lucas, &approval).await;
    assert_eq!(detail(&paul, &draft).await["skill"]["latestVersion"], 1);

    // …and a new skill of its name is proposed as any new skill: a version
    // of it, for its administrators to decide.
    let new_skill = json!({
        "name": "relance-client",
        "description": "Relancer un client par téléphone.",
        "content": "1. Appeler le client.",
        "rationale": "Utile.",
    });
    let read_as_new = ScriptedStep::dynamic(|req| {
        let results = last_tool_results(req);
        assert!(!results[0].is_error, "{}", results[0].content);
        assert!(
            !results[0].content.contains("version"),
            "{}",
            results[0].content
        );
        ScriptedReply::text("Compétence proposée.")
    });
    let (run, approval) = proposed(&api, &lucas, new_skill, read_as_new).await;
    validate(&api, &lucas, &approval).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let summary: Option<String> = sqlx::query_scalar(
        "select output_summary from tool_invocations where run_id = $1 and status = 'succeeded'",
    )
    .bind(run)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        summary.as_deref(),
        Some("Compétence « relance-client » proposée")
    );
    let shown = detail(&paul, &draft).await;
    assert_eq!(shown["skill"]["status"], "draft");
    assert_eq!(
        shown["skill"]["description"],
        "Relancer un client dont l’intervention attend."
    );
    assert_eq!(shown["skill"]["proposals"], 1);
    assert_eq!(shown["versions"][0]["version"], 2);
    assert_eq!(shown["versions"][0]["status"], "candidate");
    assert_eq!(
        shown["versions"][0]["proposedDescription"],
        "Relancer un client par téléphone."
    );
    let deleted = paul.delete(&format!("{SKILLS}/{draft}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}
