//! The connector starts an administration asks for are bounded per company
//! (ADR-055): while two are under way (tests of a saved connector, trials of
//! a server), another is refused, after the permission, the parameters and
//! the connector's existence are checked (an absent one is still answered
//! as such, and traced); a slot is freed when its start ends, not when its
//! client leaves.

mod support;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::connectors::{MAX_ADMIN_STARTS, TOO_MANY_ADMIN_STARTS};
use mimorya_connectors::testing::{Hold, serve_demo_docs_http, serve_held_mcp_http};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::mcp::{PRIVATE_DEPLOYMENT, add, mcp_body};
use support::refusals::{latest_entry, not_found_since};
use support::{Client, TestApi, TestResponse};
use uuid::Uuid;

const TRIAL: &str = "/admin/connectors/trial";

fn trial(url: &str) -> Value {
    json!({ "prefix": "docs", "url": url })
}

type Answer<'a> = Pin<Box<dyn Future<Output = TestResponse> + 'a>>;

/// Two trials held by the server: the first one's client leaves (its trial
/// goes on), the second one's answer is returned, to await once released.
async fn two_held<'a>(admin: &'a Client<'a>, url: &str, hold: &Hold) -> Answer<'a> {
    let mut first: Answer<'a> = Box::pin(admin.post(TRIAL, trial(url)));
    let mut second: Answer<'a> = Box::pin(admin.post(TRIAL, trial(url)));
    let reached = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::select! {
            answered = &mut first => panic!("answered while held: {}", answered.text()),
            () = hold.reached() => {}
        }
        tokio::select! {
            answered = &mut first => panic!("answered while held: {}", answered.text()),
            answered = &mut second => panic!("answered while held: {}", answered.text()),
            () = hold.reached() => {}
        }
    });
    assert!(reached.await.is_ok(), "the server was never reached twice");
    drop(first);
    second
}

async fn release(api: &TestApi, hold: Arc<Hold>, second: Answer<'_>) {
    hold.release();
    let answered = second.await;
    assert_eq!(answered.status, StatusCode::OK, "{}", answered.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_has_two_administration_starts_under_way_at_most() {
    assert_eq!(MAX_ADMIN_STARTS, 2);
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let answering = serve_demo_docs_http(None).await;
    let saved = add(&paul, mcp_body("Limite", "limite", &answering, None)).await;
    let test_of = |id: &str| format!("/admin/connectors/{id}/test");

    let (held, hold) = serve_held_mcp_http().await;
    let second = two_held(&paul, &held, &hold).await;

    // A third start, a trial or a test, is refused meanwhile.
    for refused in [
        paul.post(TRIAL, trial(&answering)).await,
        paul.post(&test_of(&saved), json!({})).await,
    ] {
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
        assert_eq!(refused.error_message(), TOO_MANY_ADMIN_STARTS);
    }
    // What is checked first is answered as before: a connector absent (and
    // traced as such), a member without the permission, an invalid trial.
    let since = latest_entry(&api).await;
    let absent = paul
        .post(&test_of(&Uuid::new_v4().to_string()), json!({}))
        .await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND, "{}", absent.text());
    assert_eq!(not_found_since(&api, since).await.len(), 1);
    let member = marc.post(TRIAL, trial(&answering)).await;
    assert_eq!(member.status, StatusCode::FORBIDDEN, "{}", member.text());
    let invalid = paul.post(TRIAL, trial("pas une adresse")).await;
    assert_eq!(
        invalid.status,
        StatusCode::BAD_REQUEST,
        "{}",
        invalid.text()
    );

    // Once they end, both slots are free again: two more starts are held.
    release(&api, hold, second).await;
    let (held, hold) = serve_held_mcp_http().await;
    let second = two_held(&paul, &held, &hold).await;
    release(&api, hold, second).await;

    let tested = paul.post(&test_of(&saved), json!({})).await;
    assert_eq!(tested.status, StatusCode::OK, "{}", tested.text());
    assert_eq!(tested.json()["ok"], true);
    let deleted = paul.delete(&format!("/admin/connectors/{saved}")).await;
    assert!(deleted.status.is_success(), "{}", deleted.text());
}
