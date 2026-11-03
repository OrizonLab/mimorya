//! The other administrators of the access policies told by e-mail of the
//! allow policies proposed (ADR-170), by the periodic pass: one e-mail to
//! each administrator counting the proposals another made, naming their
//! authors, never their description; the author is never told of their
//! own; each proposal announced once, audited; one nobody else may decide
//! waits for another administrator; one decided before the pass is not
//! announced; an administrator who chose not to receive these is not sent
//! one, audited as such; nothing without a relay.

mod support;

use mimorya_api::services::policy_proposal_notice::{
    POLICY_PROPOSALS_ANNOUNCED, announce_policy_proposals,
};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::notification_choices::{choose, chosen};
use support::policy_proposals::{completed_interventions, propose, retire, second_admin, withdraw};
use support::{TestApi, WEB_ORIGIN, email, owner_url};
use uuid::Uuid;

/// The subject of the notice of one proposal.
const ONE: &str = "Une autorisation proposée attend votre décision";

/// Announces the proposals, as a process that executes runs does every
/// minute; how many.
async fn announce(api: &TestApi) -> usize {
    api.app.services.runs.start_executing().unwrap();
    announce_policy_proposals(&api.app.services).await.unwrap()
}

/// The addresses the notices of one proposal went to.
fn told(api: &TestApi) -> Vec<String> {
    api.mail
        .sent()
        .into_iter()
        .filter(|m| m.subject == ONE)
        .map(|m| m.to)
        .collect()
}

/// The notices audited: recipient, outcome and metadata; forgotten after.
async fn notices_and_forget() -> Vec<(Option<Uuid>, String, Value)> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let rows = sqlx::query_as(
        "select on_behalf_of_user_id, outcome, metadata from audit_log \
         where tenant_id = $1 and action = $2 and actor_type = 'system' order by id",
    )
    .bind(tenant::ID)
    .bind(POLICY_PROPOSALS_ANNOUNCED)
    .fetch_all(&owner)
    .await
    .unwrap();
    sqlx::query("delete from audit_log where action = $1")
        .bind(POLICY_PROPOSALS_ANNOUNCED)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    rows
}

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
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let announced: bool =
        sqlx::query_scalar("select announced_at is not null from policy_proposals where id = $1")
            .bind(id.parse::<Uuid>().unwrap())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert!(!announced);
    withdraw(&paul, &id).await;
    retire(&paul, claire_id).await;
}
