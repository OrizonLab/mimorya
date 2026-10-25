//! Skills the agent proposes (ADR-100): once the person validated its
//! text, a proposal is a candidate version among the skill's, or a new
//! skill's first, which runs do not read before an administrator makes it
//! active. An administrator turns one down; only a proposal awaiting a
//! decision is. A name taken otherwise, or too many proposals waiting,
//! refuse it. Each step is audited without the procedure's text. Who
//! reaches the administration's routes: `skill_access.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_core::demo::{tenant, user};
use mimorya_core::skill_proposals::MAX_PENDING_PROPOSALS;
use mimorya_db::repositories::skills::{NewSkillVersion, get_active_skill, insert_skill_version};
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use support::refusals::latest_entry;
use support::{Client, TestApi};
use uuid::Uuid;

const SKILL: &str = "synthese-hebdo-interventions";
const SKILLS: &str = "/admin/skills";
const RATIONALE: &str = "Ajoute la relecture des interventions en retard.";

fn version_of_the_report() -> Value {
    json!({
        "name": SKILL,
        "content": "# Synthèse\r\n1. Lister les interventions.\r\n2. Relire les retards.\r\n\r\n",
        "rationale": RATIONALE,
    })
}

/// A turn of Lucas's in which the agent proposes `input`, then reads its
/// result with `then`; the run, paused on his validation, and the request.
async fn proposed(
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
    let approvals = lucas.get("/approvals").await.json();
    let request = approvals["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["runId"] == run.as_str())
        .unwrap_or_else(|| panic!("the request of {run}: {approvals}"));
    assert_eq!(request["toolName"], "skill_propose");
    (run.parse().unwrap(), request["id"].as_str().unwrap().into())
}

/// Lucas validates `approval`; the run then ends.
async fn validate(api: &TestApi, lucas: &Client<'_>, approval: &str) {
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
fn read_proposed() -> ScriptedStep {
    ScriptedStep::dynamic(|req| {
        let results = last_tool_results(req);
        assert!(!results[0].is_error, "{}", results[0].content);
        ScriptedReply::text("Proposition envoyée aux administrateurs.")
    })
}

/// The model reads a refusal saying `why`.
fn read_refused(why: &'static str) -> ScriptedStep {
    ScriptedStep::dynamic(move |req| {
        let results = last_tool_results(req);
        assert!(results[0].is_error, "{}", results[0].content);
        assert!(results[0].content.contains(why), "{}", results[0].content);
        ScriptedReply::text("La proposition est refusée.")
    })
}

/// The weekly report as the seed left it, for the tests after: its first
/// version alone, active.
async fn restore_the_report(api: &TestApi) {
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

async fn detail(paul: &Client<'_>, id: &str) -> Value {
    let shown = paul.get(&format!("{SKILLS}/{id}")).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    shown.json()
}

/// The skills' audit entries written after `since`: action and metadata.
async fn audited(api: &TestApi, since: i64) -> Vec<(String, String, Option<Uuid>, Value)> {
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
    let lucas_name: String = {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        sqlx::query_scalar("select display_name from users where id = $1")
            .bind(user::LUCAS)
            .fetch_one(&mut *tx)
            .await
            .unwrap()
    };
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
    assert_eq!(
        detail(&paul, id).await["versions"][0]["status"],
        "candidate"
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
    let deleted = paul.delete(&format!("{SKILLS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_name_taken_otherwise_or_too_many_waiting_refuse_a_proposal() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let paul = api.sign_in(user::PAUL).await;

    // A skill of the company, never active: its name is taken.
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
    let taken = json!({
        "name": "relance-client",
        "description": "Relancer un client par téléphone.",
        "content": "1. Appeler le client.",
        "rationale": "Utile.",
    });
    let (run, approval) = proposed(&api, &lucas, taken, read_refused("sans être active")).await;
    validate(&api, &lucas, &approval).await;
    assert_eq!(detail(&paul, &draft).await["skill"]["latestVersion"], 1);

    // As many proposals waiting as the company takes: no more.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    for _ in 0..MAX_PENDING_PROPOSALS {
        let waiting = NewSkillVersion {
            tenant_id: tenant::ID,
            skill_id: DEMO_SKILL_ID,
            content: "1. Une proposition.",
            changelog: "En attente.",
            created_by: user::LUCAS,
            proposed_by_run: Some(run),
        };
        insert_skill_version(&mut tx, waiting).await.unwrap();
    }
    tx.commit().await.unwrap();
    let (_, approval) = proposed(
        &api,
        &lucas,
        version_of_the_report(),
        read_refused("attendent déjà"),
    )
    .await;
    validate(&api, &lucas, &approval).await;
    let shown = detail(&paul, &DEMO_SKILL_ID.to_string()).await;
    assert_eq!(shown["skill"]["proposals"], MAX_PENDING_PROPOSALS);
    assert_eq!(
        shown["skill"]["latestVersion"],
        1 + MAX_PENDING_PROPOSALS as i32
    );
    restore_the_report(&api).await;
    let deleted = paul.delete(&format!("{SKILLS}/{draft}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}
