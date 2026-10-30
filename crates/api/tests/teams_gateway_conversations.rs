//! A linked person talks with the agent in Teams (ADR-140): their words to
//! the bot start a run as them, in a conversation of theirs held in their
//! personal conversation with it, which their next words follow until they
//! ask for a new one (`/nouvelle`), or it is archived in the application.
//! The run's end is written there, in Teams' Markdown, with the link to its
//! conversation, at the Teams service the message came from; a pause says
//! where to validate, and the answer follows once given. Commands say how
//! to talk with the agent, starting nothing. Nothing is started for
//! someone who cannot act, nor while the service stops, and why is said
//! there.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::FakeTeams;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::manager_approvals::{listed, without};
use support::planning::{apply_proposal, propose};
use support::refusals::{entries_since, latest_entry};
use support::slack_conversations::{answer, settle};
use support::sso::SSO_DEPLOYMENT;
use support::teams::MARC_TEAMS;
use support::teams_conversations::{
    last_said, linked_teams, open_teams_conversation, said_to, talk, teams_runs,
};
use support::{TestApi, WEB_ORIGIN};

/// What the model was last asked.
fn last_asked(api: &TestApi) -> String {
    let requests = api.model.as_ref().unwrap().requests();
    format!("{:?}", requests.last().expect("a request").messages)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_linked_person_talks_with_the_agent_in_their_personal_conversation() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let since = latest_entry(&api).await;
    let open = || open_teams_conversation(&api, id, MARC_TEAMS.0, user::MARC);
    let said = || last_said(&teams, MARC_TEAMS.0);

    // His words: a new conversation, held in his personal conversation.
    api.script(vec![ScriptedStep::text("**Trois** visites demain à Lyon.")]);
    let asked = "Quelles visites demain <Lyon> ?";
    talk(&api, &teams, id, "a-10", MARC_TEAMS, asked).await;
    assert!(last_asked(&api).contains(asked));
    let conversation = open().await.expect("its conversation");
    let shown = marc
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    assert_eq!(shown["conversation"]["title"], asked);
    assert_eq!(shown["conversation"]["gateway"], "teams", "{shown}");
    assert_eq!(shown["messages"][0]["metadata"]["gateway"], "teams");
    let runs = teams_runs(&api).await;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].2, "completed");
    // Answered at the Teams service his message came from.
    assert_eq!(
        thread_service(&api, conversation).await,
        teams.service_url()
    );

    // Its end, written there in Teams' Markdown, once.
    assert_eq!(answer(&api).await, 1);
    let link = format!("[ouvrir la conversation]({WEB_ORIGIN}/chat/{conversation})");
    assert_eq!(
        said(),
        format!("**Trois** visites demain à Lyon\\.\n\n*Dans l’application : {link}\\.*")
    );
    let written = said_to(&teams, MARC_TEAMS.0).pop().unwrap();
    assert_eq!(written.text_format.as_deref(), Some("markdown"));
    assert_eq!(answer(&api).await, 0);

    // His next words follow the conversation.
    api.script(vec![ScriptedStep::text("Villeurbanne ajoutée.")]);
    talk(&api, &teams, id, "a-11", MARC_TEAMS, "Ajoute Villeurbanne").await;
    assert!(
        last_asked(&api).contains("Trois"),
        "its history is replayed"
    );
    assert_eq!(open().await, Some(conversation));
    let shown = marc
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    assert_eq!(shown["messages"].as_array().unwrap().len(), 4, "{shown}");
    assert_eq!(answer(&api).await, 1);
    assert!(said().starts_with("Villeurbanne ajoutée\\."), "{}", said());

    // `/nouvelle` closes it: his next words open another.
    talk(&api, &teams, id, "a-12", MARC_TEAMS, "/nouvelle").await;
    assert_eq!(
        said(),
        "C’est noté : votre prochain message ouvrira une nouvelle conversation\\."
    );
    assert_eq!(open().await, None);
    api.script(vec![ScriptedStep::text("Bonjour Marc.")]);
    talk(&api, &teams, id, "a-13", MARC_TEAMS, "Bonjour").await;
    let second = open().await.expect("another conversation");
    assert_ne!(second, conversation);
    assert_eq!(answer(&api).await, 1);
    assert!(said().starts_with("Bonjour Marc\\."), "{}", said());

    // With words after it, `/nouvelle` opens the next one with them.
    api.script(vec![ScriptedStep::text("Deux visites.")]);
    talk(
        &api,
        &teams,
        id,
        "a-14",
        MARC_TEAMS,
        "/nouvelle Et demain ?",
    )
    .await;
    let third = open().await.expect("a third conversation");
    assert_ne!(third, second);
    let shown = marc.get(&format!("/conversations/{third}")).await.json();
    assert_eq!(shown["conversation"]["title"], "Et demain ?");
    assert_eq!(shown["messages"][0]["content"], "Et demain ?");

    // Any other command, or no words, says how to talk here, starting
    // nothing.
    for (activity_id, command) in [("a-15", "/aide"), ("a-16", "/inconnue"), ("a-17", " ")] {
        talk(&api, &teams, id, activity_id, MARC_TEAMS, command).await;
        assert!(
            said().starts_with("Votre compte Teams est relié au compte de Marc Lefèvre"),
            "{}",
            said()
        );
        assert!(said().contains("/nouvelle"), "{}", said());
    }
    assert_eq!(teams_runs(&api).await.len(), 4);
    assert_eq!(open().await, Some(third));

    // Audited as his own turns, never what he wrote.
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("agent.run.started"), "{audited}");
    assert!(audited.contains(&id.to_string()), "{audited}");
    assert!(!audited.contains("Villeurbanne"), "{audited}");
}

/// The Teams service the thread holding `conversation` answers at.
async fn thread_service(api: &TestApi, conversation: uuid::Uuid) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let service =
        sqlx::query_scalar("select service_url from gateway_threads where conversation_id = $1")
            .bind(conversation)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    service
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_conversation_archived_in_the_application_is_left_for_a_new_one() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    api.script(vec![ScriptedStep::text("Première réponse.")]);
    talk(&api, &teams, id, "a-10", MARC_TEAMS, "Première question").await;
    let first = open_teams_conversation(&api, id, MARC_TEAMS.0, user::MARC)
        .await
        .expect("its conversation");
    let archived = marc
        .patch(
            &format!("/conversations/{first}"),
            json!({ "archived": true }),
        )
        .await;
    assert_eq!(archived.status, StatusCode::OK, "{}", archived.text());

    api.script(vec![ScriptedStep::text("Seconde réponse.")]);
    talk(&api, &teams, id, "a-11", MARC_TEAMS, "Seconde question").await;
    let second = open_teams_conversation(&api, id, MARC_TEAMS.0, user::MARC)
        .await
        .expect("a new conversation");
    assert_ne!(second, first);
    let kept = marc.get(&format!("/conversations/{first}")).await.json();
    assert_eq!(kept["messages"].as_array().unwrap().len(), 2, "{kept}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pause_for_validation_is_said_then_the_answer_follows() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let said = || last_said(&teams, MARC_TEAMS.0);
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal(""),
        ScriptedStep::text("Planning de INT-2052 appliqué."),
    ]);
    talk(&api, &teams, id, "a-10", MARC_TEAMS, "Planifie INT-2052").await;
    let (run, conversation, status) = teams_runs(&api).await.remove(0);
    assert_eq!(status, "awaiting_approval");
    assert_eq!(answer(&api).await, 1);
    let paused = said();
    assert!(
        paused.starts_with(
            "Une action de cette demande attend votre validation, à donner dans l’application"
        ),
        "{paused}"
    );
    assert!(
        paused.contains(&format!("{WEB_ORIGIN}/chat/{conversation}")),
        "{paused}"
    );

    // Meanwhile, the conversation is busy: said there.
    talk(&api, &teams, id, "a-11", MARC_TEAMS, "Et INT-2053 ?").await;
    assert!(said().contains("déjà en cours"), "{}", said());
    assert_eq!(teams_runs(&api).await.len(), 1);

    // Validated in the application, the run's end follows.
    let request = listed(&marc, &run.to_string()).await.expect("his request");
    let decided = marc
        .post(
            &format!("/approvals/{}/decision", request["id"].as_str().unwrap()),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    settle(&api).await;
    assert_eq!(answer(&api).await, 1);
    assert!(
        said().starts_with("Planning de INT\\-2052 appliqué\\."),
        "{}",
        said()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_is_started_for_someone_who_cannot_act_and_why_is_said() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let said = || last_said(&teams, MARC_TEAMS.0);

    // Without using the agent.
    let policy = without(&paul, user::MARC, "agent:use").await;
    talk(&api, &teams, id, "a-10", MARC_TEAMS, "Bonjour").await;
    assert_eq!(said(), "Permission requise : agent:use");
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);

    // While the service stops.
    api.app.begin_shutdown();
    talk(&api, &teams, id, "a-11", MARC_TEAMS, "Bonjour").await;
    assert!(said().starts_with("Le service redémarre"), "{}", said());

    // Nothing started, no conversation left.
    assert!(teams_runs(&api).await.is_empty());
    assert_eq!(
        open_teams_conversation(&api, id, MARC_TEAMS.0, user::MARC).await,
        None
    );
}
