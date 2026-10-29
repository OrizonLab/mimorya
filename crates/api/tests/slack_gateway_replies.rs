//! What becomes of the answers in Slack (ADR-127): each end or pause of a
//! run started there is answered once; a paused gateway, or a Slack user no
//! longer the person's identity, is written nothing; what Slack refuses is
//! not tried again: a pause whose buttons it refuses is said without them,
//! once, but not one it refuses otherwise (ADR-129, ADR-130); Slack
//! unreachable is tried again later, a few times, then given up; a
//! thread's answers are written in order, an answer too old to try again
//! holding back none.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::gateways::MAX_ATTEMPTS;
use mimorya_connectors::testing::FakeSlack;
use mimorya_core::demo::user;
use serde_json::json;
use support::gateways::ACCOUNT;
use support::manager_approvals::listed;
use support::planning::{apply_proposal, propose};
use support::slack_conversations::{
    MARC_SLACK, answer, in_thread, linked, replies_of, retry_now, settle, slack_runs, write,
    written,
};
use support::slack_validations::{ASKED_IN, asked, posts_in};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email, owner_url};

const SECOND: &str = "1700000300.000100";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_paused_gateway_or_a_removed_identity_is_written_nothing() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;

    // Paused before its run's end is answered.
    api.script(vec![ScriptedStep::text("Réponse A.")]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question A", ASKED_IN, None),
    )
    .await;
    let path = format!("/admin/gateways/{gateway}");
    let paused = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, ASKED_IN).is_empty());
    assert_eq!(
        replies_of(&api, ASKED_IN).await,
        [(Some("skipped".into()), 1)]
    );
    // Enabled again: not written later.
    let resumed = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(resumed.status, StatusCode::OK, "{}", resumed.text());
    assert_eq!(answer(&api).await, 0);

    // Unlinked by the person before the end is answered.
    api.script(vec![ScriptedStep::text("Réponse B.")]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question B", SECOND, None),
    )
    .await;
    let removed = marc.delete(&format!("{ACCOUNT}/{gateway}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, SECOND).is_empty());
    assert_eq!(
        replies_of(&api, SECOND).await,
        [(Some("skipped".into()), 1)]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_slack_refuses_is_not_tried_again() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.script(vec![ScriptedStep::text("Réponse.")]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question", ASKED_IN, None),
    )
    .await;

    slack.refuse_with(Some("channel_not_found"));
    assert_eq!(answer(&api).await, 0);
    slack.refuse_with(None);
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, ASKED_IN).is_empty());
    assert_eq!(
        replies_of(&api, ASKED_IN).await,
        [(Some("refused".into()), 1)]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pause_whose_buttons_slack_refuses_is_said_without_them() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    asked(&api, (gateway, MARC_SLACK), "Appliqué.").await;

    slack.refuse_blocks(true);
    assert_eq!(answer(&api).await, 1);
    slack.refuse_blocks(false);
    let said = posts_in(&slack, ASKED_IN);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].blocks, None);
    assert!(
        said[0]
            .text
            .starts_with("Une action de cette demande attend une validation avant de continuer"),
        "{}",
        said[0].text
    );
    assert!(
        said[0].text.contains("|ouvrir la conversation>"),
        "{}",
        said[0].text
    );
    assert_eq!(replies_of(&api, ASKED_IN).await, [(Some("sent".into()), 1)]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pause_slack_refuses_otherwise_is_not_said_again_without_buttons() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    asked(&api, (gateway, MARC_SLACK), "Appliqué.").await;

    let tried = slack.post_attempts();
    slack.refuse_posts_with(Some("channel_not_found"));
    assert_eq!(answer(&api).await, 0);
    slack.refuse_posts_with(None);
    assert_eq!(answer(&api).await, 0);
    assert_eq!(slack.post_attempts(), tried + 1, "tried once");
    assert!(posts_in(&slack, ASKED_IN).is_empty());
    assert_eq!(
        replies_of(&api, ASKED_IN).await,
        [(Some("refused".into()), 1)]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn slack_unreachable_is_tried_again_later_a_few_times() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.script(vec![
        ScriptedStep::text("Réponse A."),
        ScriptedStep::text("Réponse B."),
    ]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question A", ASKED_IN, None),
    )
    .await;

    // Down: kept to try again, not before its time.
    slack.unavailable(true);
    assert_eq!(answer(&api).await, 0);
    assert_eq!(replies_of(&api, ASKED_IN).await, [(None, 1)]);
    slack.unavailable(false);
    assert_eq!(answer(&api).await, 0);
    // Its time come, Slack up again: written.
    retry_now(&api, ASKED_IN).await;
    assert_eq!(answer(&api).await, 1);
    assert_eq!(in_thread(&slack, ASKED_IN).len(), 1);
    assert_eq!(replies_of(&api, ASKED_IN).await, [(Some("sent".into()), 2)]);

    // Down at every attempt: given up.
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question B", SECOND, None),
    )
    .await;
    slack.unavailable(true);
    for _ in 0..MAX_ATTEMPTS {
        assert_eq!(answer(&api).await, 0);
        retry_now(&api, SECOND).await;
    }
    slack.unavailable(false);
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, SECOND).is_empty());
    assert_eq!(
        replies_of(&api, SECOND).await,
        [(Some("failed".into()), MAX_ATTEMPTS)]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_thread_s_answers_are_written_in_order() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal(""),
        ScriptedStep::text("Planning de INT-2052 appliqué."),
    ]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Planifie INT-2052", ASKED_IN, None),
    )
    .await;
    // Validated before any pass: its pause and its end, both to answer.
    let run = slack_runs(&api).await.remove(0).0;
    let request = listed(&marc, &run.to_string()).await.expect("his request");
    let id = request["id"].as_str().unwrap();
    let decided = marc
        .post(
            &format!("/approvals/{id}/decision"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    settle(&api).await;

    // Slack down: the pause is tried, its end waits for it, in the same pass.
    slack.unavailable(true);
    assert_eq!(answer(&api).await, 0);
    assert_eq!(replies_of(&api, ASKED_IN).await, [(None, 1)]);
    slack.unavailable(false);
    // And at the next ones, while the pause is still to try again.
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, ASKED_IN).is_empty());
    // The pause's time come: the pause, then its end at the next pass.
    retry_now(&api, ASKED_IN).await;
    assert_eq!(answer(&api).await, 1);
    assert_eq!(in_thread(&slack, ASKED_IN).len(), 1);
    assert_eq!(answer(&api).await, 1);
    let said = in_thread(&slack, ASKED_IN);
    assert!(said[0].contains("attend une validation"), "{said:?}");
    assert!(
        said[1].starts_with("Planning de INT-2052 appliqué."),
        "{said:?}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_answer_too_old_to_try_again_holds_its_thread_back_no_more() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.script(vec![
        ScriptedStep::text("Réponse A."),
        ScriptedStep::text("Réponse B."),
    ]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question A", ASKED_IN, None),
    )
    .await;
    slack.unavailable(true);
    assert_eq!(answer(&api).await, 0);
    slack.unavailable(false);

    // A day later, its time come, it is no longer tried; the person's next
    // question in the thread is answered.
    age_ends(ASKED_IN, 25).await;
    retry_now(&api, ASKED_IN).await;
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question B", SECOND, Some(ASKED_IN)),
    )
    .await;
    assert_eq!(answer(&api).await, 1);
    let said = in_thread(&slack, ASKED_IN);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].starts_with("Réponse B."), "{said:?}");
    assert_eq!(
        replies_of(&api, ASKED_IN).await,
        [(None, 1), (Some("sent".into()), 1)]
    );
}

/// Moves the ends and pauses answered in the thread `thread` `hours`
/// earlier, as the database's owner.
async fn age_ends(thread: &str, hours: i32) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "update run_events set created_at = created_at - make_interval(hours => $2) \
         where id in (select r.event_id from gateway_replies r \
         join gateway_threads t on t.id = r.thread_id where t.thread_ts = $1)",
    )
    .bind(thread)
    .bind(hours)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
