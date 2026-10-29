//! Validations given in Slack (ADR-129): a run started there that pauses
//! for its author's own validation shows the action with its exact
//! parameters and its buttons; pressed, it is decided as its author,
//! audited so, the message showing the outcome in place of the buttons, and
//! the run goes on. Only whom the request awaits decides it: another linked
//! person, a Slack user not linked, one no longer the person, another
//! workspace, a paused gateway or an unsigned press decide nothing (a
//! temporary password, a suspended company: `slack_gateway_validation_checks`).
//! An action a manager decides, one sent outside the company or whose
//! parameters do not fit is shown without buttons, and no press decides it.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeSlack, OTHER_WORKSPACE, SlackPost, WORKSPACE};
use mimorya_core::demo::user;
use mimorya_core::gateways::slack::{ShownAction, approval_blocks};
use serde_json::json;
use support::manager_approvals::{listed, route_to_managers};
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use support::slack_conversations::{
    MARC_SLACK, answer, confirm, in_thread, link_code, linked, settle,
};
use support::slack_validations::{
    ASKED_IN, asked, buttons_of, moved_to, paused, posts_in, press, pressed, pressed_by,
};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email};

const LEA_SLACK: &str = "U0LEAFONT";
const NADIA_SLACK: &str = "U0NADIABEN";

/// `post` with buttons for `approval` made up, as a press may claim.
fn with_buttons(post: &SlackPost, approval: uuid::Uuid) -> SlackPost {
    let parameters = json!({});
    let action = ShownAction {
        approval_id: approval,
        summary: "Appliquer",
        risk: "write",
        parameters: &parameters,
    };
    let mut shown = post.clone();
    shown.blocks = Some(json!(approval_blocks("Pause", &[action])));
    shown
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_awaiting_its_author_is_validated_with_its_button() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (run, pause) = paused(
        &api,
        &slack,
        (gateway, MARC_SLACK),
        "Planning de INT-2052 appliqué.",
    )
    .await;
    // The pause: the action, with its buttons.
    assert!(
        pause.text.contains("attend votre validation"),
        "{}",
        pause.text
    );
    let request = listed(&marc, &run).await.expect("his request");
    assert_eq!(
        buttons_of(&pause).to_string(),
        request["id"].as_str().unwrap()
    );
    let blocks = pause.blocks.clone().unwrap().to_string();
    assert!(blocks.contains("Action à valider"), "{blocks}");

    // Validated in Slack: decided as him, audited so, shown in the message.
    let since = latest_entry(&api).await;
    let sent = press(&api, gateway, &pressed_by(MARC_SLACK, &pause, true), false).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    settle(&api).await;
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "approved");
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("approval.approved"), "{audited}");
    assert!(audited.contains(r#""gateway":"slack""#), "{audited}");
    let shown = posts_in(&slack, ASKED_IN).remove(0);
    assert_eq!(shown.updates, 1);
    let blocks = shown.blocks.unwrap().to_string();
    assert!(
        blocks.contains("*Validée* par vous dans Slack. La demande reprend"),
        "{blocks}"
    );
    assert!(!blocks.contains("approval_approve"), "{blocks}");
    // The run went on: its end follows in the thread.
    assert_eq!(answer(&api).await, 1);
    let said = in_thread(&slack, ASKED_IN);
    assert!(
        said[1].starts_with("Planning de INT-2052 appliqué."),
        "{said:?}"
    );

    // Pressed again: nothing more, and said.
    press(&api, gateway, &pressed_by(MARC_SLACK, &pause, true), false).await;
    settle(&api).await;
    let said = in_thread(&slack, ASKED_IN);
    assert_eq!(said.last().unwrap(), "Cette demande a déjà été traitée");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_refused_with_its_button_is_not_carried_out() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (run, pause) = paused(&api, &slack, (gateway, MARC_SLACK), "Planning inchangé.").await;
    let sent = press(&api, gateway, &pressed_by(MARC_SLACK, &pause, false), false).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    settle(&api).await;
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "rejected");
    let blocks = posts_in(&slack, ASKED_IN)
        .remove(0)
        .blocks
        .unwrap()
        .to_string();
    assert!(
        blocks.contains("*Refusée* par vous dans Slack."),
        "{blocks}"
    );
    assert_eq!(answer(&api).await, 1);
    assert!(in_thread(&slack, ASKED_IN)[1].starts_with("Planning inchangé."));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_whom_the_request_awaits_decides_it_from_slack() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (run, pause) = paused(
        &api,
        &slack,
        (gateway, MARC_SLACK),
        "Planning de INT-2052 appliqué.",
    )
    .await;
    let pending = || async { listed(&marc, &run).await.unwrap()["status"].clone() };
    let said = |slack: &FakeSlack| slack.posts().last().unwrap().text.clone();
    let before = slack.posts().len();

    // Unsigned: as to an unknown gateway.
    let body = pressed_by(MARC_SLACK, &pause, true);
    assert_eq!(
        press(&api, gateway, &body, true).await.status,
        StatusCode::NOT_FOUND
    );
    // Of another workspace, or while the gateway is paused: nothing.
    let foreign = pressed(
        OTHER_WORKSPACE,
        MARC_SLACK,
        &pause,
        buttons_of(&pause),
        true,
    );
    assert_eq!(
        press(&api, gateway, &foreign, false).await.status,
        StatusCode::OK
    );
    let path = format!("/admin/gateways/{gateway}");
    assert_eq!(
        paul.patch(&path, json!({ "enabled": false })).await.status,
        StatusCode::OK
    );
    assert_eq!(
        press(&api, gateway, &body, false).await.status,
        StatusCode::OK
    );
    assert_eq!(
        paul.patch(&path, json!({ "enabled": true })).await.status,
        StatusCode::OK
    );
    settle(&api).await;
    assert_eq!(pending().await, "pending");
    assert_eq!(slack.posts().len(), before, "nothing said");

    // Another linked person, in his conversation: nothing said there (ADR-131).
    let lea = api.sign_in(user::LEA).await;
    slack.add_person(LEA_SLACK, &email(user::LEA));
    let code = link_code(&api, &slack, gateway, LEA_SLACK).await;
    confirm(&lea, &code).await;
    let before = slack.posts().len();
    press(&api, gateway, &pressed_by(LEA_SLACK, &pause, true), false).await;
    settle(&api).await;
    assert_eq!(slack.posts().len(), before, "nothing said");
    // In hers: nothing she may decide.
    let hers = moved_to(&pause, LEA_SLACK);
    press(&api, gateway, &pressed_by(LEA_SLACK, &hers, true), false).await;
    settle(&api).await;
    assert_eq!(said(&slack), "Demande de validation introuvable");
    // A Slack user not linked: told so, and offered a link.
    slack.add_person("U0INCONNU", "inconnu@neo-maintenance.example");
    let theirs = moved_to(&pause, "U0INCONNU");
    press(
        &api,
        gateway,
        &pressed_by("U0INCONNU", &theirs, true),
        false,
    )
    .await;
    settle(&api).await;
    let posts = slack.posts();
    assert!(
        posts[posts.len() - 2].text.contains("n’est pas relié"),
        "{posts:?}"
    );
    assert!(said(&slack).contains("gateway-link#"), "{posts:?}");
    // His own Slack user, no longer him for Slack: unlinked, nothing decided.
    slack.add_person(MARC_SLACK, &email(user::LEA));
    press(&api, gateway, &pressed_by(MARC_SLACK, &pause, true), false).await;
    settle(&api).await;
    assert_eq!(pending().await, "pending");
    let own = marc.get("/account/gateways").await.json();
    assert_eq!(own["gateways"][0]["linked"], serde_json::Value::Null);
    assert_eq!(posts_in(&slack, ASKED_IN)[0].updates, 0, "its buttons left");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_a_manager_decides_is_shown_without_buttons() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, nadia) = (
        api.sign_in(user::PAUL).await,
        api.sign_in(user::NADIA).await,
    );
    let address = email(user::NADIA);
    let gateway = linked(&api, &slack, &paul, &nadia, (NADIA_SLACK, &address)).await;
    // Her manager, Marc, validates what she writes.
    route_to_managers(&paul, &["write"]).await;
    let (run, pause) = paused(&api, &slack, (gateway, NADIA_SLACK), "Appliqué.").await;
    route_to_managers(&paul, &[]).await;
    assert_eq!(pause.blocks, None);
    assert!(
        pause.text.contains("validation de votre responsable"),
        "{}",
        pause.text
    );

    // Her pressing a button made for it, either one, decides nothing: she
    // withdraws it in the application.
    let request = listed(&nadia, &run).await.expect("her request");
    let approval = request["id"].as_str().unwrap().parse().unwrap();
    let shown = with_buttons(&pause, approval);
    for approved in [true, false] {
        let before = slack.posts().len();
        let body = pressed(WORKSPACE, NADIA_SLACK, &shown, approval, approved);
        press(&api, gateway, &body, false).await;
        settle(&api).await;
        let said: Vec<_> = slack.posts().split_off(before);
        assert_eq!(said.len(), 1, "{said:?}");
        assert_eq!(
            said[0].text,
            "Cette action attend la validation de votre responsable : rien n’a été décidé."
        );
        assert_eq!(listed(&nadia, &run).await.unwrap()["status"], "pending");
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_sent_outside_the_company_is_validated_in_the_application() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let run = asked(&api, (gateway, MARC_SLACK), "Appliqué.").await;
    let request = listed(&marc, &run).await.expect("his request");
    let approval = request["id"].as_str().unwrap().parse().unwrap();
    // As an e-mail sent in his name would be.
    as_owner(
        "update approvals set risk = 'external' where id = $1",
        approval,
    )
    .await;
    assert_eq!(answer(&api).await, 1);
    let pause = posts_in(&slack, ASKED_IN).remove(0);
    assert_eq!(pause.blocks, None);
    assert!(
        pause.text.contains("à donner dans l’application"),
        "{}",
        pause.text
    );

    // A press claiming its buttons decides nothing.
    let shown = with_buttons(&pause, approval);
    let body = pressed(WORKSPACE, MARC_SLACK, &shown, approval, true);
    press(&api, gateway, &body, false).await;
    settle(&api).await;
    let said = slack.posts().last().unwrap().text.clone();
    assert!(
        said.starts_with("Cette action se valide dans l’application"),
        "{said}"
    );
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
    // Nor one naming a request that is not his, or none.
    let unknown = uuid::Uuid::new_v4();
    let body = pressed(
        WORKSPACE,
        MARC_SLACK,
        &with_buttons(&pause, unknown),
        unknown,
        true,
    );
    press(&api, gateway, &body, false).await;
    settle(&api).await;
    let said = slack.posts().last().unwrap().text.clone();
    assert_eq!(said, "Demande de validation introuvable");
}
