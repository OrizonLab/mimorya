//! Telling a mission's author that a stage of it awaits their validation
//! (ADR-091), as a scheduled task's author is (ADR-082): once per pause,
//! naming the mission and its conversation, never the actions nor their
//! parameters; audited per request. One a stop of the service lost is sent
//! ten minutes on, once, by the periodic catch-up.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::author_notices::unnotified_mission_runs;
use serde_json::json;
use support::manager_approvals::{catch_up, paused_ago};
use support::mission_notices::{awaiting, told};
use support::missions::{executing, read, stop};
use support::scheduled_notices::notices;
use support::{TestApi, WEB_ORIGIN, email};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_author_is_told_once_where_their_mission_waits() {
    let api = TestApi::start_with_mailer().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let title = "Mission à valider";
    let (id, _, approval) = awaiting(&api, &marc, title).await;

    let sent = told(&api, title);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::MARC));
    assert_eq!(
        sent[0].subject,
        "Votre mission « Mission à valider » attend votre validation"
    );
    let conversation = read(&marc, id).await["conversationId"].clone();
    let link = format!("{WEB_ORIGIN}/chat/{}", conversation.as_str().unwrap());
    assert!(sent[0].text.contains(&link), "{}", sent[0].text);
    assert!(
        sent[0]
            .text
            .contains("mène en arrière-plan, demande votre validation")
    );
    assert!(sent[0].text.contains("une action (risque : modification)"));
    assert!(!sent[0].text.contains("INT-2049"));
    let approval_id = approval["id"].as_str().unwrap();
    assert_eq!(
        notices(&api, tenant::ID, approval_id).await,
        [(
            user::MARC,
            "success".to_string(),
            json!({ "approverId": user::MARC, "grouped": 1 })
        )]
    );

    // Not again: the catch-up finds nothing left to tell.
    paused_ago(&api, tenant::ID, approval_id.parse().unwrap(), 11).await;
    catch_up(&api).await;
    assert_eq!(told(&api, title).len(), 1);
    assert_eq!(stop(&marc, id).await, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_notice_a_stop_lost_is_sent_ten_minutes_on_once() {
    // Paused by a process that could not e-mail: what a stop between the
    // pause and its notice leaves.
    let stopped = TestApi::start().await;
    executing(&stopped);
    let marc = stopped.sign_in(user::MARC).await;
    let title = "Mission à valider, avis perdu";
    let (id, run, approval) = awaiting(&stopped, &marc, title).await;
    let approval_id: Uuid = approval["id"].as_str().unwrap().parse().unwrap();
    let started = TestApi::start_with_mailer().await;
    let pool = started.app.services.db.pool();
    let listed = unnotified_mission_runs(pool, Utc::now()).await.unwrap();
    assert!(
        listed
            .iter()
            .any(|r| r.run_id == run && r.tenant_id == tenant::ID)
    );

    // Paused less than ten minutes ago: maybe under way, left alone.
    paused_ago(&started, tenant::ID, approval_id, 9).await;
    catch_up(&started).await;
    assert!(told(&started, title).is_empty());
    paused_ago(&started, tenant::ID, approval_id, 11).await;
    catch_up(&started).await;
    let sent = told(&started, title);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::MARC));
    assert_eq!(
        sent[0].subject,
        "Votre mission « Mission à valider, avis perdu » attend votre validation"
    );

    catch_up(&started).await;
    assert_eq!(told(&started, title).len(), 1);
    let listed = unnotified_mission_runs(pool, Utc::now()).await.unwrap();
    assert!(!listed.iter().any(|r| r.run_id == run));
    assert_eq!(stop(&marc, id).await, StatusCode::OK);
}
