//! Requests awaiting a manager told in Slack (ADR-130): the manager linked
//! to the company's Slack application is written a direct message of a
//! paused run's requests awaiting them, once, with the buttons of those
//! Slack may validate; pressed, the request is decided as that manager,
//! audited so, the message showing the outcome. While the manager is away,
//! whom they named to stand in for them is told instead, and decides. An
//! action sent outside the company is told without buttons, and no press
//! decides it; nor does a press of another person's. A manager who lost
//! the right to decide is written nothing; a stand-in who could not decide
//! leaves the manager told.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::gateways::NOTIFIED_IN_SLACK;
use mimorya_connectors::testing::{FakeSlack, WORKSPACE};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::delegations::{away, back, julien_decides_for_the_south};
use support::manager_approvals::{listed, without};
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use support::slack_approvers::{direct_to, nadia_s_request, notice_of, notify, refuse};
use support::slack_conversations::{MARC_SLACK, confirm, link_code, linked, settle};
use support::slack_validations::{buttons_of, moved_to, press, pressed, pressed_by};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email};
use uuid::Uuid;

const JULIEN_SLACK: &str = "U0JULIENMO";
const LEA_SLACK: &str = "U0LEAFONT";

/// The metadata of each notice in Slack audited for the request.
async fn audited(api: &TestApi, approval: Uuid) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_scalar(
        "select metadata from audit_log where action = $1 and resource_id = $2 order by id",
    )
    .bind(NOTIFIED_IN_SLACK)
    .bind(approval.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_manager_is_told_in_slack_and_validates_with_a_button() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;

    // Told once, in his direct conversation, with the buttons.
    assert_eq!(notify(&api).await, 1);
    assert_eq!(notify(&api).await, 0);
    let told = direct_to(&slack, MARC_SLACK);
    assert_eq!(told.len(), 1, "{told:?}");
    let notice = &told[0];
    assert!(
        notice
            .text
            .starts_with("*Nadia Benali* : l’agent attend votre validation pour une action."),
        "{}",
        notice.text
    );
    assert_eq!(buttons_of(notice), approval);
    let blocks = notice.blocks.clone().unwrap().to_string();
    assert!(blocks.contains("proposalId"), "{blocks}");
    assert_eq!(
        notice_of(&api, approval).await,
        Some((Some("sent".into()), 1))
    );
    let manager = json!({ "gatewayId": gateway, "approverId": user::MARC, "grouped": 1 });
    assert_eq!(audited(&api, approval).await, [manager]);

    // Validated in Slack: decided as him, audited so, shown in the message.
    let since = latest_entry(&api).await;
    let sent = press(&api, gateway, &pressed_by(MARC_SLACK, notice, true), false).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    settle(&api).await;
    let decided = listed(&marc, &run).await.expect("the request he decided");
    assert_eq!(decided["status"], "approved");
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("approval.approved"), "{audited}");
    assert!(audited.contains(r#""gateway":"slack""#), "{audited}");
    let shown = direct_to(&slack, MARC_SLACK).remove(0);
    assert_eq!(shown.updates, 1);
    let blocks = shown.blocks.unwrap().to_string();
    assert!(
        blocks.contains("*Validée* par vous dans Slack. La demande reprend."),
        "{blocks}"
    );
    assert!(!blocks.contains("la suite viendra"), "{blocks}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_stand_in_is_told_and_decides_in_the_manager_s_absence() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let julien = api.sign_in(user::JULIEN).await;
    let assignment = julien_decides_for_the_south(&paul).await;
    away(&marc, user::JULIEN, 0).await;
    let address = email(user::JULIEN);
    let gateway = linked(&api, &slack, &paul, &julien, (JULIEN_SLACK, &address)).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;

    assert_eq!(notify(&api).await, 1);
    let told = direct_to(&slack, JULIEN_SLACK);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(
        told[0].text.contains(", en l’absence de Marc Lefèvre."),
        "{}",
        told[0].text
    );
    let audit = json!({
        "gatewayId": gateway,
        "approverId": user::MARC,
        "grouped": 1,
        "delegateId": user::JULIEN,
    });
    assert_eq!(audited(&api, approval).await, [audit]);
    press(
        &api,
        gateway,
        &pressed_by(JULIEN_SLACK, &told[0], false),
        false,
    )
    .await;
    settle(&api).await;
    let decided = listed(&julien, &run).await.expect("the request he decided");
    assert_eq!(decided["status"], "rejected");
    assert_eq!(decided["decidedBy"]["id"], json!(user::JULIEN));

    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_sent_outside_the_company_is_told_without_buttons() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;
    // As an e-mail sent in her name would be.
    as_owner(
        "update approvals set risk = 'external' where id = $1",
        approval,
    )
    .await;

    assert_eq!(notify(&api).await, 1);
    let notice = direct_to(&slack, MARC_SLACK).remove(0);
    assert_eq!(notice.blocks, None);
    assert!(
        notice
            .text
            .contains("Elle se valide dans l’application, qui en montre les paramètres exacts"),
        "{}",
        notice.text
    );
    assert!(notice.text.contains("/approvals|"), "{}", notice.text);

    // A press claiming its buttons decides nothing.
    let mut shown = notice.clone();
    shown.blocks = Some(json!(mimorya_core::gateways::slack::approval_blocks(
        "Pause",
        &[mimorya_core::gateways::ShownAction {
            approval_id: approval,
            summary: "Envoyer",
            risk: "write",
            parameters: &json!({}),
        }]
    )));
    press(&api, gateway, &pressed_by(MARC_SLACK, &shown, true), false).await;
    settle(&api).await;
    let said = slack.posts().last().unwrap().text.clone();
    assert!(
        said.starts_with("Cette action se valide dans l’application"),
        "{said}"
    );
    // Nor one by another linked person.
    let lea = api.sign_in(user::LEA).await;
    slack.add_person(LEA_SLACK, &email(user::LEA));
    let code = link_code(&api, &slack, gateway, LEA_SLACK).await;
    confirm(&lea, &code).await;
    let body = pressed(
        WORKSPACE,
        LEA_SLACK,
        &moved_to(&shown, LEA_SLACK),
        approval,
        true,
    );
    press(&api, gateway, &body, false).await;
    settle(&api).await;
    let said = slack.posts().last().unwrap().text.clone();
    assert_eq!(said, "Demande de validation introuvable");
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
    refuse(&api, &marc, approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_who_lost_the_right_is_written_nothing() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;

    // Named when the request was made, he loses the right before its
    // notice: neither its parameters nor its buttons are written to him.
    let policy = without(&paul, user::MARC, "approval:decide").await;
    assert_eq!(notify(&api).await, 0);
    assert!(direct_to(&slack, MARC_SLACK).is_empty());
    assert_eq!(
        notice_of(&api, approval).await,
        Some((Some("skipped".into()), 1))
    );
    assert!(audited(&api, approval).await.is_empty());
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    refuse(&api, &marc, approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stand_in_who_could_not_decide_leaves_the_manager_told() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    // Julien stands in for Marc, without approval:decide over Nadia.
    away(&marc, user::JULIEN, 0).await;
    let address = email(user::MARC);
    linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;

    assert_eq!(notify(&api).await, 1);
    let told = direct_to(&slack, MARC_SLACK);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(
        !told[0].text.contains("en l’absence de"),
        "{}",
        told[0].text
    );
    back(&marc).await;
    refuse(&api, &marc, approval).await;
}
