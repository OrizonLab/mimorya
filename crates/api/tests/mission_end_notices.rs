//! Telling a mission's author that it is over (ADR-091), by the periodic
//! pass: done, saying how its answer was reviewed, or unfinished, whichever
//! path ended it (past its limits, the stale-run sweep); once, audited,
//! never its answer, by one process that executes runs at a time, with a
//! relay. Not when its author stopped it, nor while their account is
//! disabled, nor a day after its end; a message the relay refuses is
//! audited and not sent again. Each company's missions are read in it
//! alone.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::plugins::maintenance::sweep_stale_runs;
use mimorya_api::services::missions::{TOLD_WITHIN, tell_ended_missions};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::mission_end_notices::{mission_end_notice, unnotified_mission_ends};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::mission_notices::{end_notices, end_notified, ended_ago, tell_ends, told};
use support::missions::{
    approving_review, entrust, executing, limit, read, spending_stage, stages, stop,
};
use support::queue::take_queued;
use support::{Client, TestApi, WEB_ORIGIN, email, owner_url};
use uuid::Uuid;

const ANSWER: &str = "Synthèse : aucune intervention ne reste sans suivi cette semaine.";
const NOTE: &str = "Fait : la semaine relue. Reste : proposer le suivi.";

/// Entrusts `client`'s mission `title` and has it answer at once, its answer
/// reviewed with `review`: the mission, done.
async fn done(api: &TestApi, client: &Client<'_>, title: &str, review: ScriptedStep) -> Uuid {
    let id = entrust(client, title).await;
    api.script(vec![ScriptedStep::text(ANSWER), review]);
    assert_eq!(take_queued(api).await, 1);
    assert_eq!(read(client, id).await["status"], "done");
    id
}

fn audited(status: &str) -> Vec<(String, Value)> {
    vec![("success".into(), json!({ "status": status }))]
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_done_is_told_to_its_author_once() {
    let api = TestApi::start_with_mailer().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let title = "Mission menée à bien";
    let id = done(&api, &marc, title, approving_review()).await;
    assert!(told(&api, title).is_empty());

    tell_ends(&api).await;
    let sent = told(&api, title);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::MARC));
    assert_eq!(
        sent[0].subject,
        "Votre mission « Mission menée à bien » est terminée"
    );
    assert!(
        sent[0]
            .text
            .contains("Sa réponse finale a été vérifiée au regard de ses critères de réussite.")
    );
    assert!(sent[0].text.contains(&format!("{WEB_ORIGIN}/missions\n")));
    assert!(!sent[0].text.contains(ANSWER));
    assert_eq!(end_notices(id).await, audited("done"));

    tell_ends(&api).await;
    assert_eq!(told(&api, title).len(), 1);
    assert_eq!(end_notices(id).await.len(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_answer_no_reviewer_judged_is_not_told_as_verified() {
    let api = TestApi::start_with_mailer().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let title = "Mission sans relecture";
    let unjudged = ScriptedStep::text("Je ne peux pas me prononcer.");
    done(&api, &marc, title, unjudged).await;

    tell_ends(&api).await;
    let sent = told(&api, title);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].text.contains(
        "Sa réponse finale n’a pas pu être relue au regard de ses critères de réussite."
    ));
    assert!(!sent[0].text.contains("a été vérifiée"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_process_that_executes_runs_with_a_relay_tells_of_it_once() {
    let quiet = TestApi::start().await;
    executing(&quiet);
    let marc = quiet.sign_in(user::MARC).await;
    let title = "Mission annoncée une seule fois";
    let id = done(&quiet, &marc, title, approving_review()).await;
    // Without a relay: nothing sent, nothing recorded, so that it is told
    // of once one is configured.
    assert_eq!(tell_ended_missions(&quiet.app.services).await.unwrap(), 0);
    assert_eq!(end_notified(id).await, None);

    let (one, two) = (
        TestApi::start_with_mailer().await,
        TestApi::start_with_mailer().await,
    );
    // A process that executes no runs tells of nothing.
    assert_eq!(tell_ended_missions(&one.app.services).await.unwrap(), 0);
    assert!(one.mail.sent().is_empty());
    // Under way elsewhere: skipped, nobody told. Held before either
    // executes runs, so that no maintenance pass of theirs takes it first.
    let held = one
        .app
        .services
        .db
        .try_sweep_lock("mission-end-notices")
        .await;
    let held = held.unwrap().expect("free");
    for worker in [&one, &two] {
        worker.app.services.runs.start_executing().unwrap();
    }
    assert_eq!(tell_ended_missions(&two.app.services).await.unwrap(), 0);
    assert!(two.mail.sent().is_empty());
    assert_eq!(end_notified(id).await, None);
    held.release().await;

    let (first, second) = tokio::join!(
        tell_ended_missions(&one.app.services),
        tell_ended_missions(&two.app.services)
    );
    first.unwrap();
    second.unwrap();
    assert_eq!(told(&one, title).len() + told(&two, title).len(), 1);
    assert_eq!(end_notices(id).await, audited("done"));
    assert!(end_notified(id).await.is_some());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_unfinished_is_told_whichever_path_ended_it() {
    let api = TestApi::start_with_mailer().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;

    // Past its limits: ended as its stage hands over.
    let limited = "Mission à court d’étapes";
    let past_limits = entrust(&marc, limited).await;
    limit(past_limits, 1, 12_000_000).await;
    api.script(spending_stage(NOTE));
    assert_eq!(take_queued(&api).await, 1);
    assert_eq!(read(&marc, past_limits).await["status"], "failed");

    // Its stage stuck, then failed by the stale-run sweep: still in
    // progress, it reads failed.
    let stuck = "Mission restée bloquée";
    let swept = entrust(&marc, stuck).await;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "update agent_runs set status = 'running', updated_at = now() - interval '20 minutes' \
         where id = $1",
    )
    .bind(stages(swept).await[0].0)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    let services = &api.app.services;
    let failed = sweep_stale_runs(&services.db, &services.bus, Duration::minutes(15))
        .await
        .unwrap();
    assert_eq!(failed, 1);
    assert_eq!(read(&marc, swept).await["status"], "failed");

    tell_ends(&api).await;
    for (title, id) in [(limited, past_limits), (stuck, swept)] {
        let sent = told(&api, title);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(
            sent[0].subject,
            format!("Votre mission « {title} » s’est arrêtée inachevée")
        );
        assert!(sent[0].text.contains(&format!("{WEB_ORIGIN}/missions\n")));
        assert!(!sent[0].text.contains(NOTE));
        assert_eq!(end_notices(id).await, audited("failed"));
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nobody_is_told_of_a_mission_stopped_long_over_or_whose_author_is_disabled() {
    let api = TestApi::start_with_mailer().await;
    executing(&api);
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let nadia = api.sign_in(user::NADIA).await;

    // Stopped by its author: they know.
    let stopped = "Mission arrêtée par Marc";
    let id = entrust(&marc, stopped).await;
    assert_eq!(stop(&marc, id).await, StatusCode::OK);
    // Over for more than a day.
    let old = "Mission terminée avant-hier";
    let long_over = done(&api, &marc, old, approving_review()).await;
    ended_ago(long_over, 25).await;
    // Its author disabled since it ended.
    let away = "Mission de Nadia";
    let hers = done(&api, &nadia, away, approving_review()).await;
    let account = format!("/admin/users/{}", user::NADIA);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());

    tell_ends(&api).await;
    for title in [stopped, old, away] {
        assert!(told(&api, title).is_empty(), "{title}");
    }
    // Read in her company, as it would be once listed: not to tell of.
    let since = Utc::now() - TOLD_WITHIN;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    assert!(
        mission_end_notice(&mut tx, hers, since)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();
    assert!(end_notices(id).await.is_empty());
    assert!(end_notices(long_over).await.is_empty());

    // Back, within a day of its end: she is told.
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    tell_ends(&api).await;
    let sent = told(&api, away);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::NADIA));
    assert_eq!(end_notices(hers).await, audited("done"));
    assert!(told(&api, stopped).is_empty() && told(&api, old).is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_message_the_relay_refuses_is_audited_and_not_sent_again() {
    let api = TestApi::start_with_mailer().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let title = "Mission au relais injoignable";
    let id = done(&api, &marc, title, approving_review()).await;
    api.mail.fail();

    tell_ends(&api).await;
    let refused = vec![("failure".to_string(), json!({ "status": "done" }))];
    assert_eq!(end_notices(id).await, refused);
    tell_ends(&api).await;
    assert_eq!(end_notices(id).await, refused);
    assert!(told(&api, title).is_empty());
}

/// A mission of the other company, done: its identifier.
async fn foreign_mission_done() -> Uuid {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let id = sqlx::query_scalar(
        "with c as (insert into conversations (tenant_id, owner_user_id, title) \
         values ($1, $2, 'Mission étrangère') returning id) \
         insert into missions (tenant_id, owner_user_id, conversation_id, title, description, \
         success_criteria, status, limits, finished_at) select $1, $2, c.id, \
         'Mission étrangère', 'Objectif', '[\"Critère\"]'::jsonb, 'done', \
         '{\"maxStages\": 6, \"maxCostMicros\": 12000000}'::jsonb, now() from c returning id",
    )
    .bind(OTHER_TENANT_ID)
    .bind(OTHER_USER_ID)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    id
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_s_missions_are_read_in_it_alone() {
    let api = TestApi::start().await;
    let foreign = foreign_mission_done().await;
    let db = &api.app.services.db;
    let since = Utc::now() - TOLD_WITHIN;

    let ended = unnotified_mission_ends(db.pool(), since).await.unwrap();
    let listed = ended.iter().find(|e| e.mission_id == foreign).unwrap();
    assert_eq!(listed.tenant_id, OTHER_TENANT_ID);
    // From the demo company, the other's mission is not there to tell of.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert!(
        mission_end_notice(&mut tx, foreign, since)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();
    let mut tx = db.begin(OTHER_TENANT_ID).await.unwrap();
    let notice = mission_end_notice(&mut tx, foreign, since).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(notice.unwrap().owner_user_id, OTHER_USER_ID);

    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "delete from conversations where id = (select conversation_id from missions where id = $1)",
    )
    .bind(foreign)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
