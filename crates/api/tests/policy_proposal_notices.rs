//! The other administrators of the access policies told by e-mail of the
//! allow policies proposed (ADR-170), by the periodic pass: one e-mail to
//! each administrator counting the proposals another made, naming their
//! authors, never their description; the author is never told of their
//! own; each proposal announced once, audited; one nobody else may decide
//! waits for another administrator; one decided before the pass is not
//! announced; an administrator who chose not to receive these is not sent
//! one, audited as such; a message the relay refuses audited and not sent
//! again; nothing without a relay nor in a process that executes no runs,
//! one process at a time. Who is told: `policy_proposal_notice_recipients.rs`.

mod support;

use std::time::Duration;

use mimorya_api::services::policy_proposal_notice::{
    POLICY_PROPOSAL_NOTICE_SWEEP, announce_policy_proposals,
};
use mimorya_core::demo::user;
use serde_json::json;
use support::notification_choices::{choose, chosen};
use support::policy_proposal_notices::{ONE, announce, announced, notices_and_forget, told};
use support::policy_proposals::{completed_interventions, propose, retire, second_admin, withdraw};
use support::{TestApi, WEB_ORIGIN, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_is_announced_once_to_the_other_administrators_never_to_its_author() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (claire, claire_id) = second_admin(&api, &paul, "claire.avis").await;
    let id = propose(&paul, completed_interventions()).await;
    assert!(told(&api).is_empty());

    assert_eq!(announce(&api).await, 1);
    let sent: Vec<_> = api
        .mail
        .sent()
        .into_iter()
        .filter(|m| m.subject == ONE)
        .collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, "claire.avis@neo-maintenance.example");
    assert!(
        sent[0]
            .text
            .contains("Paul Renaud propose une politique d’autorisation")
    );
    assert!(sent[0].text.contains(&format!("{WEB_ORIGIN}/admin\n")));
    // Never what the policy says.
    assert!(
        !sent[0].text.contains("interventions terminées"),
        "{}",
        sent[0].text
    );
    assert_eq!(
        notices_and_forget().await,
        [(
            Some(claire_id),
            "success".to_string(),
            json!({ "proposals": [id] })
        )]
    );

    // Once.
    assert_eq!(announce(&api).await, 0);
    assert_eq!(told(&api).len(), 1);
    // Claire's own proposal is told to Paul, not to her.
    let hers = propose(&claire, completed_interventions()).await;
    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api).last().unwrap(), &email(user::PAUL));
    assert_eq!(told(&api).len(), 2);
    notices_and_forget().await;

    withdraw(&paul, &id).await;
    withdraw(&claire, &hers).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_nobody_else_may_decide_waits_and_a_decided_one_is_not_announced() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    // Paul alone administers the policies: nobody to tell.
    let waiting = propose(&paul, completed_interventions()).await;
    let decided = propose(&paul, completed_interventions()).await;
    assert_eq!(announce(&api).await, 0);
    withdraw(&paul, &decided).await;

    // Another administrator arrives: the waiting one is told, alone.
    let (_, claire_id) = second_admin(&api, &paul, "claire.attente").await;
    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api), ["claire.attente@neo-maintenance.example"]);
    assert_eq!(
        notices_and_forget().await,
        [(
            Some(claire_id),
            "success".to_string(),
            json!({ "proposals": [waiting] })
        )]
    );
    withdraw(&paul, &waiting).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_who_chose_not_to_be_told_is_sent_nothing() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (claire, claire_id) = second_admin(&api, &paul, "claire.silence").await;
    choose(&claire, chosen(false, true, true)).await;
    let id = propose(&paul, completed_interventions()).await;

    assert_eq!(announce(&api).await, 1);
    assert!(told(&api).is_empty());
    assert_eq!(
        notices_and_forget().await,
        [(
            Some(claire_id),
            "denied".to_string(),
            json!({ "proposals": [id], "optedOut": true })
        )]
    );
    // Counted as told: not sent once she chooses otherwise.
    choose(&claire, chosen(true, true, true)).await;
    assert_eq!(announce(&api).await, 0);
    assert!(told(&api).is_empty());
    withdraw(&paul, &id).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_is_announced_without_a_relay() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, claire_id) = second_admin(&api, &paul, "claire.relais").await;
    let id = propose(&paul, completed_interventions()).await;
    assert_eq!(announce(&api).await, 0);
    assert!(!announced(&api, &id).await);
    withdraw(&paul, &id).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_message_the_relay_refuses_is_audited_and_not_sent_again() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, claire_id) = second_admin(&api, &paul, "claire.refus").await;
    let id = propose(&paul, completed_interventions()).await;
    api.mail.fail();

    assert_eq!(announce(&api).await, 1);
    assert!(told(&api).is_empty());
    assert!(announced(&api, &id).await);
    assert_eq!(
        notices_and_forget().await,
        [(
            Some(claire_id),
            "failure".to_string(),
            json!({ "proposals": [id] })
        )]
    );
    // Not sent again.
    assert_eq!(announce(&api).await, 0);
    assert!(notices_and_forget().await.is_empty());
    assert!(told(&api).is_empty());
    withdraw(&paul, &id).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_process_that_executes_runs_announces_at_a_time() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, claire_id) = second_admin(&api, &paul, "claire.verrou").await;
    let id = propose(&paul, completed_interventions()).await;
    // A process that executes no runs announces nothing.
    let services = &api.app.services;
    assert_eq!(announce_policy_proposals(services).await.unwrap(), 0);
    // Under way elsewhere: skipped, nothing announced.
    let held = services
        .db
        .try_sweep_lock(POLICY_PROPOSAL_NOTICE_SWEEP)
        .await;
    let held = held.unwrap().expect("free");
    assert_eq!(announce(&api).await, 0);
    assert!(told(&api).is_empty());
    assert!(!announced(&api, &id).await);
    held.release().await;

    announce(&api).await;
    // A maintenance pass may have announced it meanwhile: once it is done.
    let wait = Duration::from_secs(5);
    let settled = services.db.sweep_lock(POLICY_PROPOSAL_NOTICE_SWEEP, wait);
    settled.await.unwrap().release().await;
    assert_eq!(told(&api), ["claire.verrou@neo-maintenance.example"]);
    assert!(announced(&api, &id).await);
    notices_and_forget().await;
    withdraw(&paul, &id).await;
    retire(&paul, claire_id).await;
}
