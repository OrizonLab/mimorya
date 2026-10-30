//! A linked person talks with the agent in Telegram (ADR-135): their words
//! to the bot start a run as them, in a conversation of theirs held in
//! their private chat with it, which their next words follow until they
//! ask for a new one (`/nouvelle`), or it is archived in the application.
//! The run's end is written there, in HTML, with the link to its
//! conversation; a pause says where to validate, and the answer follows
//! once given. Commands say how to talk with the agent, starting nothing.
//! Nothing is started for someone who cannot act, nor in a chat whose
//! conversation another message opened meanwhile, nor while the service
//! stops, and why is said in the chat. A Telegram account linked since to
//! someone else follows that person.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::AuthIdentity;
use mimorya_api::services::runs::GatewayThread;
use mimorya_connectors::testing::FakeTelegram;
use mimorya_core::DomainError;
use mimorya_core::demo::{tenant, user};
use mimorya_core::gateways::GatewayKind;
use serde_json::json;
use support::manager_approvals::{listed, without};
use support::planning::{apply_proposal, propose};
use support::refusals::{entries_since, latest_entry};
use support::slack_conversations::{answer, settle};
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{MARC_TELEGRAM, MARC_USERNAME, ask_code, confirm, open_and_accept};
use support::telegram_conversations::{
    chat_replies, last_said, linked_bot, open_conversation, talk, telegram_runs,
};
use support::{TestApi, WEB_ORIGIN, owner_url};

const MARC: (i64, &str) = (MARC_TELEGRAM, MARC_USERNAME);

/// What the model was last asked.
fn last_asked(api: &TestApi) -> String {
    let requests = api.model.as_ref().unwrap().requests();
    format!("{:?}", requests.last().expect("a request").messages)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_linked_person_talks_with_the_agent_in_their_chat() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let since = latest_entry(&api).await;
    let open = || open_conversation(&api, id, MARC_TELEGRAM, user::MARC);

    // His words: a new conversation, held in his chat, as he wrote them.
    api.script(vec![ScriptedStep::text("**Trois** visites demain à Lyon.")]);
    let asked = "Quelles visites demain <Lyon> ?";
    talk(&api, &telegram, id, 10, MARC, asked).await;
    assert!(last_asked(&api).contains(asked));
    let conversation = open().await.expect("its conversation");
    let shown = marc
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    assert_eq!(shown["conversation"]["title"], asked);
    assert_eq!(shown["conversation"]["gateway"], "telegram", "{shown}");
    assert_eq!(shown["messages"][0]["metadata"]["gateway"], "telegram");
    let runs = telegram_runs(&api).await;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].2, "completed");

    // Its end, written in his chat, in HTML, once.
    assert_eq!(answer(&api).await, 1);
    let said = last_said(&telegram, MARC_TELEGRAM);
    assert!(said.html);
    let link = format!("<a href=\"{WEB_ORIGIN}/chat/{conversation}\">ouvrir la conversation</a>");
    assert_eq!(
        said.text,
        format!("<b>Trois</b> visites demain à Lyon.\n\n<i>Dans l’application : {link}.</i>")
    );
    assert_eq!(answer(&api).await, 0);

    // His next words follow the conversation.
    api.script(vec![ScriptedStep::text("Villeurbanne ajoutée.")]);
    talk(&api, &telegram, id, 11, MARC, "Ajoute Villeurbanne").await;
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
    assert!(
        last_said(&telegram, MARC_TELEGRAM)
            .text
            .starts_with("Villeurbanne ajoutée.")
    );

    // `/nouvelle` closes it: his next words open another.
    talk(&api, &telegram, id, 12, MARC, "/nouvelle").await;
    let said = last_said(&telegram, MARC_TELEGRAM);
    assert_eq!(
        said.text,
        "C’est noté : votre prochain message ouvrira une nouvelle conversation."
    );
    assert!(!said.html);
    assert_eq!(open().await, None);
    api.script(vec![ScriptedStep::text("Bonjour Marc.")]);
    talk(&api, &telegram, id, 13, MARC, "Bonjour").await;
    let second = open().await.expect("another conversation");
    assert_ne!(second, conversation);
    assert_eq!(answer(&api).await, 1);
    assert!(
        last_said(&telegram, MARC_TELEGRAM)
            .text
            .starts_with("Bonjour Marc.")
    );

    // With words after it, `/nouvelle` opens the next one with them.
    api.script(vec![ScriptedStep::text("Deux visites.")]);
    talk(&api, &telegram, id, 14, MARC, "/nouvelle Et demain ?").await;
    let third = open().await.expect("a third conversation");
    assert_ne!(third, second);
    let shown = marc.get(&format!("/conversations/{third}")).await.json();
    assert_eq!(shown["conversation"]["title"], "Et demain ?");
    assert_eq!(shown["messages"][0]["content"], "Et demain ?");

    // Any other command says how to talk here, starting nothing.
    for (update_id, command) in [
        (15, "/aide"),
        (16, "/start"),
        (17, "/inconnue@NeoMaintenanceBot"),
    ] {
        talk(&api, &telegram, id, update_id, MARC, command).await;
        let said = last_said(&telegram, MARC_TELEGRAM).text;
        assert!(
            said.starts_with("Votre compte Telegram est relié au compte de Marc Lefèvre"),
            "{said}"
        );
        assert!(said.contains("/nouvelle"), "{said}");
    }
    assert_eq!(telegram_runs(&api).await.len(), 4);
    assert_eq!(open().await, Some(third));

    // Audited as his own turns, never what he wrote.
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("agent.run.started"), "{audited}");
    assert!(audited.contains(&id.to_string()), "{audited}");
    assert!(!audited.contains("Villeurbanne"), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_conversation_archived_in_the_application_is_left_for_a_new_one() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    api.script(vec![ScriptedStep::text("Première réponse.")]);
    talk(&api, &telegram, id, 10, MARC, "Première question").await;
    let first = open_conversation(&api, id, MARC_TELEGRAM, user::MARC)
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
    talk(&api, &telegram, id, 11, MARC, "Seconde question").await;
    let second = open_conversation(&api, id, MARC_TELEGRAM, user::MARC)
        .await
        .expect("a new conversation");
    assert_ne!(second, first);
    // The archived one is left as it was.
    let kept = marc.get(&format!("/conversations/{first}")).await.json();
    assert_eq!(kept["messages"].as_array().unwrap().len(), 2, "{kept}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pause_for_validation_is_said_then_the_answer_follows() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal(""),
        ScriptedStep::text("Planning de INT-2052 appliqué."),
    ]);
    talk(&api, &telegram, id, 10, MARC, "Planifie INT-2052").await;
    let (run, conversation, status) = telegram_runs(&api).await.remove(0);
    assert_eq!(status, "awaiting_approval");
    assert_eq!(answer(&api).await, 1);
    let paused = last_said(&telegram, MARC_TELEGRAM).text;
    assert!(
        paused.starts_with(
            "Une action de cette demande attend votre validation : validez-la ou refusez-la \
             ci-dessous"
        ),
        "{paused}"
    );
    assert!(
        paused.contains(&format!("{WEB_ORIGIN}/chat/{conversation}")),
        "{paused}"
    );

    // Meanwhile, the conversation is busy: said in the chat.
    talk(&api, &telegram, id, 11, MARC, "Et INT-2053 ?").await;
    let busy = last_said(&telegram, MARC_TELEGRAM);
    assert!(busy.text.contains("déjà en cours"), "{}", busy.text);
    assert_eq!(telegram_runs(&api).await.len(), 1);

    // Validated in the application, the run's end follows in the chat.
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
    let said = last_said(&telegram, MARC_TELEGRAM).text;
    assert!(said.starts_with("Planning de INT-2052 appliqué."), "{said}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_is_started_for_someone_who_cannot_act_and_why_is_said() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let said = || last_said(&telegram, MARC_TELEGRAM).text;

    // Without using the agent.
    let policy = without(&paul, user::MARC, "agent:use").await;
    talk(&api, &telegram, id, 10, MARC, "Bonjour").await;
    assert_eq!(said(), "Permission requise : agent:use");
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);

    // With a temporary password still to replace.
    let temporary = "update users set must_change_password = $2 where id = $1";
    owned(temporary, user::MARC, true).await;
    talk(&api, &telegram, id, 11, MARC, "Bonjour").await;
    owned(temporary, user::MARC, false).await;
    assert!(said().contains("mot de passe provisoire"), "{}", said());

    // His company suspended.
    let suspended = "update tenants set status = case when $2 then 'suspended' else 'active' end \
         where id = $1";
    owned(suspended, tenant::ID, true).await;
    api.app.services.access.invalidate(tenant::ID);
    talk(&api, &telegram, id, 12, MARC, "Bonjour").await;
    owned(suspended, tenant::ID, false).await;
    api.app.services.access.invalidate(tenant::ID);
    assert!(
        said().starts_with("Votre entreprise est suspendue"),
        "{}",
        said()
    );

    // While the service stops.
    api.app.begin_shutdown();
    talk(&api, &telegram, id, 13, MARC, "Bonjour").await;
    assert!(said().starts_with("Le service redémarre"), "{}", said());

    // Nothing started, no conversation left.
    assert!(telegram_runs(&api).await.is_empty());
    assert_eq!(
        open_conversation(&api, id, MARC_TELEGRAM, user::MARC).await,
        None
    );
}

/// Runs `sql` on `id` and `value` as the database's owner.
async fn owned(sql: &str, id: uuid::Uuid, value: bool) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(sql)
        .bind(id)
        .bind(value)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_telegram_account_linked_since_to_someone_else_follows_them() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let lea = api.sign_in(user::LEA).await;
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    api.script(vec![ScriptedStep::text("Réponse pour Marc.")]);
    talk(&api, &telegram, id, 10, MARC, "Question de Marc").await;
    let his = open_conversation(&api, id, MARC_TELEGRAM, user::MARC)
        .await
        .expect("his conversation");

    // He removes his link; that Telegram account is linked to Léa since.
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    let code = ask_code(&lea, id).await;
    open_and_accept(&api, &telegram, id, 11, MARC, &code).await;
    assert_eq!(confirm(&lea, id).await.status, StatusCode::OK);

    // `/nouvelle` from that account closes nothing of his.
    talk(&api, &telegram, id, 13, MARC, "/nouvelle").await;
    let still = open_conversation(&api, id, MARC_TELEGRAM, user::MARC).await;
    assert_eq!(still, Some(his));

    // His run's end is not written there.
    let before = telegram.messages().len();
    assert_eq!(answer(&api).await, 0);
    assert_eq!(telegram.messages().len(), before);
    assert_eq!(
        chat_replies(&api, MARC_TELEGRAM).await,
        [(Some("skipped".into()), 1)]
    );
    // Its words follow Léa now, in a conversation of hers.
    api.script(vec![ScriptedStep::text("Réponse pour Léa.")]);
    talk(&api, &telegram, id, 14, MARC, "Question de Léa").await;
    let hers = open_conversation(&api, id, MARC_TELEGRAM, user::LEA)
        .await
        .expect("her conversation");
    assert_ne!(hers, his);
    let shown = lea.get(&format!("/conversations/{hers}")).await.json();
    assert_eq!(shown["messages"][0]["content"], "Question de Léa");
    let kept = marc.get(&format!("/conversations/{his}")).await.json();
    assert_eq!(kept["messages"].as_array().unwrap().len(), 2, "{kept}");
    assert_eq!(answer(&api).await, 1);
    let said = last_said(&telegram, MARC_TELEGRAM).text;
    assert!(said.starts_with("Réponse pour Léa."), "{said}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_chat_opened_meanwhile_is_refused_as_busy() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    // Two messages finding no open conversation, the second started as
    // the first opened one: refused, nothing of it kept.
    let identity = AuthIdentity {
        tenant_id: tenant::ID,
        user_id: user::MARC,
        session_id: uuid::Uuid::nil(),
        must_change_password: false,
    };
    let chat = MARC_TELEGRAM.to_string();
    let opened_by = |message: &'static str| GatewayThread {
        gateway_id: id,
        kind: GatewayKind::Telegram,
        external_id: &chat,
        channel_id: &chat,
        thread_ts: message,
        service_url: None,
    };
    let runs = &api.app.services.runs;
    api.script(vec![ScriptedStep::text("Réponse.")]);
    let title = "Conversation ouverte deux fois";
    runs.start_in_thread(identity, opened_by("10"), title, "Bonjour")
        .await
        .unwrap();
    let refused = runs
        .start_in_thread(identity, opened_by("11"), title, "Bonjour encore")
        .await
        .unwrap_err();
    let refusal = refused.downcast_ref::<DomainError>().expect("a refusal");
    assert!(
        refusal.message.contains("déjà en cours"),
        "{}",
        refusal.message
    );
    settle(&api).await;
    let listed = marc.get("/conversations").await.json();
    let titled = listed["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["title"] == title)
        .count();
    assert_eq!(titled, 1, "{listed}");
}
