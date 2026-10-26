//! E-mailing a hook's author that a run a delivery started awaits their
//! validation (ADR-111), as a scheduled task's is (ADR-082): once per pause,
//! naming the hook and the run's conversation, never the actions, their
//! parameters nor what was posted; audited per request. A notice a stop of
//! the service lost is sent by the periodic catch-up, ten minutes on; a run
//! its author follows in a conversation of theirs is told to nobody.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::author_notices::unnotified_hook_runs;
use serde_json::{Value, json};
use support::hooks::{HOOKS, deliver, hook, listed};
use support::manager_approvals::{applying, catch_up, paused_ago};
use support::planning::{apply_proposal, propose};
use support::recovery::restart;
use support::scheduled_notices::{notices, refuse};
use support::{Client, TestApi, WEB_ORIGIN, email};
use uuid::Uuid;

const TITLE: &str = "Alerte de la supervision";
const POSTED: &str = "{\"alerte\": \"pompe P2 à l’arrêt\"}";

/// Has `api` take a delivery to Nadia's hook `id`, its run asking to apply
/// a planning; the run's conversation and the request it awaits. What it
/// says once the request ends is scripted then (`end`): the steps are
/// played in order, whichever run asks.
async fn paused(
    api: &TestApi,
    nadia: &Client<'_>,
    (id, token): (Uuid, &str),
    posted: &str,
) -> (String, Value) {
    api.script(vec![propose("INT-2049", "09:00"), apply_proposal("")]);
    let accepted = deliver(api, id, token, posted).await;
    assert_eq!(accepted.status, StatusCode::ACCEPTED, "{}", accepted.text());
    api.idle().await;
    let conversation = listed(nadia, id).await["lastConversationId"]
        .as_str()
        .unwrap()
        .to_string();
    let approvals = nadia.get("/approvals?status=pending").await.json();
    let approval = approvals["approvals"][0].clone();
    assert!(approval.is_object(), "{approvals}");
    (conversation, approval)
}

/// Refuses `approval` as Nadia, its run then ending.
async fn end(api: &TestApi, nadia: &Client<'_>, approval: &Value) {
    api.script(vec![ScriptedStep::text("Inchangé.")]);
    refuse(api, nadia, approval).await;
}

/// Whether the catch-up lists `approval`'s run among the hooks' runs.
async fn listed_for_catch_up(api: &TestApi, approval: &Value) -> bool {
    let run: Uuid = approval["runId"].as_str().unwrap().parse().unwrap();
    let runs = unnotified_hook_runs(api.app.services.db.pool(), Utc::now())
        .await
        .unwrap();
    runs.iter()
        .any(|r| r.run_id == run && r.tenant_id == tenant::ID)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_author_is_told_once_where_their_hook_s_run_waits() {
    let api = TestApi::start_with_mailer().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (id, token) = hook(&nadia, TITLE).await;
    let (conversation, approval) = paused(&api, &nadia, (id, &token), POSTED).await;

    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::NADIA));
    assert_eq!(
        sent[0].subject,
        "Votre déclencheur « Alerte de la supervision » attend votre validation"
    );
    let link = format!("{WEB_ORIGIN}/chat/{conversation}");
    assert!(sent[0].text.contains(&link), "{}", sent[0].text);
    assert!(sent[0].text.contains("une action (risque : modification)"));
    // Neither the action nor what was posted leaves the platform.
    for withheld in ["INT-2049", "pompe"] {
        assert!(!sent[0].text.contains(withheld), "{}", sent[0].text);
    }
    assert_eq!(
        notices(&api, tenant::ID, approval["id"].as_str().unwrap()).await,
        [(
            user::NADIA,
            "success".to_string(),
            json!({ "approverId": user::NADIA, "grouped": 1 })
        )]
    );
    // Not again: neither at the next start, nor by the catch-up.
    restart(&api).await;
    let approval_id: Uuid = approval["id"].as_str().unwrap().parse().unwrap();
    paused_ago(&api, tenant::ID, approval_id, 11).await;
    assert_eq!(catch_up(&api).await, 0);
    assert_eq!(api.mail.sent().len(), 1);
    end(&api, &nadia, &approval).await;
    nadia.delete(&format!("{HOOKS}/{id}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_notice_a_stop_lost_is_sent_ten_minutes_on_and_none_for_a_followed_run() {
    let stopped = TestApi::start().await;
    let nadia = stopped.sign_in(user::NADIA).await;
    let (id, token) = hook(&nadia, TITLE).await;
    let (conversation, approval) = paused(&stopped, &nadia, (id, &token), POSTED).await;
    let approval_id: Uuid = approval["id"].as_str().unwrap().parse().unwrap();
    assert!(
        notices(&stopped, tenant::ID, &approval_id.to_string())
            .await
            .is_empty()
    );
    // A run she follows herself, paused as well: no hook's.
    let (_, followed) = applying(&stopped, &nadia, ("INT-2052", "10:00"), "Inchangé.").await;
    let followed = followed.unwrap();
    let followed_id: Uuid = followed["id"].as_str().unwrap().parse().unwrap();
    paused_ago(&stopped, tenant::ID, followed_id, 11).await;
    assert!(!listed_for_catch_up(&stopped, &followed).await);

    let started = TestApi::start_with_mailer().await;
    assert_eq!(restart(&started).await, 0, "no run was running");
    // Paused less than ten minutes ago: maybe under way, left alone.
    paused_ago(&started, tenant::ID, approval_id, 9).await;
    assert_eq!(catch_up(&started).await, 0);
    paused_ago(&started, tenant::ID, approval_id, 11).await;
    assert!(listed_for_catch_up(&started, &approval).await);
    assert_eq!(catch_up(&started).await, 1);
    let sent = started.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::NADIA));
    assert!(sent[0].subject.starts_with("Votre déclencheur « "));
    assert!(sent[0].text.contains(&format!("/chat/{conversation}")));
    assert!(!listed_for_catch_up(&started, &approval).await);
    assert_eq!(catch_up(&started).await, 0);
    assert_eq!(started.mail.sent().len(), 1);

    // The followed run's last words wait in the script: the first run to
    // resume says them.
    refuse(&stopped, &nadia, &approval).await;
    end(&stopped, &nadia, &followed).await;
    nadia.delete(&format!("{HOOKS}/{id}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_was_posted_names_neither_the_conversation_nor_the_e_mail() {
    let api = TestApi::start_with_mailer().await;
    let nadia = api.sign_in(user::NADIA).await;
    // Titled as a new conversation, with a short request: what follows it
    // would have titled the conversation.
    let made = nadia
        .post(
            HOOKS,
            json!({ "title": "Nouvelle conversation", "instruction": "R" }),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.text());
    let made = made.json();
    let id: Uuid = made["id"].as_str().unwrap().parse().unwrap();
    let token = made["token"].as_str().unwrap();
    let posted = "{\"code\": \"4711-AZ\"}";
    let (conversation, approval) = paused(&api, &nadia, (id, token), posted).await;
    let thread = nadia
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    assert_eq!(thread["conversation"]["title"], "Nouvelle conversation");
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(
        sent[0].subject,
        "Votre déclencheur « Nouvelle conversation » attend votre validation"
    );
    for withheld in ["4711", "donnees_outil"] {
        assert!(!sent[0].subject.contains(withheld), "{}", sent[0].subject);
        assert!(!sent[0].text.contains(withheld), "{}", sent[0].text);
    }
    end(&api, &nadia, &approval).await;

    // Her reply in that conversation, paused as well: hers, told to nobody.
    api.script(vec![propose("INT-2052", "10:00"), apply_proposal("")]);
    let replied = nadia
        .post(
            &format!("/conversations/{conversation}/messages"),
            json!({ "content": "Planifie plutôt INT-2052." }),
        )
        .await;
    assert_eq!(replied.status, StatusCode::ACCEPTED, "{}", replied.text());
    api.idle().await;
    let approvals = nadia.get("/approvals?status=pending").await.json();
    let reply = approvals["approvals"][0].clone();
    assert_eq!(reply["runId"], replied.json()["runId"], "{approvals}");
    let reply_id: Uuid = reply["id"].as_str().unwrap().parse().unwrap();
    paused_ago(&api, tenant::ID, reply_id, 11).await;
    assert!(!listed_for_catch_up(&api, &reply).await);
    assert_eq!(catch_up(&api).await, 0);
    assert_eq!(api.mail.sent().len(), 1);
    end(&api, &nadia, &reply).await;
    nadia.delete(&format!("{HOOKS}/{id}")).await;
}
