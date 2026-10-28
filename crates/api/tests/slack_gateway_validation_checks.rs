//! Who presses a button in Slack, checked before anything is decided
//! (ADR-129): a temporary password still to replace or a suspended company
//! decides nothing, and says so; Slack is asked who the Slack user is
//! before anything is said of the account, so that one no longer the
//! person is unlinked and told nothing of it.

mod support;

use mimorya_connectors::testing::FakeSlack;
use mimorya_core::demo::{tenant, user};
use support::manager_approvals::listed;
use support::queue::as_owner;
use support::slack_conversations::{MARC_SLACK, linked, settle};
use support::slack_validations::{ASKED_IN, paused, posts_in, press, pressed_by};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email};

const TEMPORARY: &str = "update users set must_change_password = true where id = $1";
const SUSPEND: &str = "update tenants set status = 'suspended' where id = $1";

/// Marc's account with a temporary password, or not.
async fn temporary_password(set: bool) {
    as_owner(&TEMPORARY.replace("true", &set.to_string()), user::MARC).await;
}

/// The demonstration company suspended, or active again.
async fn suspended(api: &TestApi, set: bool) {
    let status = if set { "'suspended'" } else { "'active'" };
    as_owner(&SUSPEND.replace("'suspended'", status), tenant::ID).await;
    api.app.services.access.invalidate(tenant::ID);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_temporary_password_or_a_suspended_company_decides_nothing() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (run, pause) = paused(&api, &slack, (gateway, MARC_SLACK), "Appliqué.").await;
    let body = pressed_by(MARC_SLACK, &pause, true);
    let said = |slack: &FakeSlack| slack.posts().last().unwrap().text.clone();

    // A temporary password still to replace.
    temporary_password(true).await;
    press(&api, gateway, &body, false).await;
    settle(&api).await;
    temporary_password(false).await;
    assert!(
        said(&slack).contains("mot de passe provisoire"),
        "{}",
        said(&slack)
    );
    assert!(
        said(&slack).ends_with("rien n’a été décidé."),
        "{}",
        said(&slack)
    );
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");

    // His company suspended.
    suspended(&api, true).await;
    press(&api, gateway, &body, false).await;
    settle(&api).await;
    suspended(&api, false).await;
    assert_eq!(
        said(&slack),
        "Votre entreprise est suspendue : l’agent n’agit plus pour elle."
    );
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
    assert_eq!(posts_in(&slack, ASKED_IN)[0].updates, 0, "its buttons left");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn slack_is_asked_before_anything_is_said_of_the_account() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (run, pause) = paused(&api, &slack, (gateway, MARC_SLACK), "Appliqué.").await;
    let before = slack.posts().len();

    // His account with a temporary password, his company suspended, and
    // his Slack user someone else's now: only that is said.
    temporary_password(true).await;
    suspended(&api, true).await;
    slack.add_person(MARC_SLACK, &email(user::LEA));
    press(&api, gateway, &pressed_by(MARC_SLACK, &pause, true), false).await;
    settle(&api).await;
    temporary_password(false).await;
    suspended(&api, false).await;
    let said: Vec<_> = (slack.posts().split_off(before).into_iter())
        .map(|p| p.text)
        .collect();
    assert_eq!(said.len(), 2, "{said:?}");
    assert!(
        said[0].contains("ne dit plus que ce compte Slack est le vôtre"),
        "{said:?}"
    );
    assert!(said[1].contains("gateway-link#"), "{said:?}");
    let told = said.join("\n");
    assert!(!told.contains("mot de passe provisoire"), "{told}");
    assert!(!told.contains("suspendue"), "{told}");
    // Unlinked, nothing decided.
    let own = marc.get("/account/gateways").await.json();
    assert_eq!(own["gateways"][0]["linked"], serde_json::Value::Null);
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
}
