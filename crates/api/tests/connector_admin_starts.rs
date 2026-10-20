//! The connector starts an administration asks for are bounded per company
//! (ADR-055): while two are under way (tests of a saved connector, trials of
//! a server), another is refused, and a slot is freed when its start ends,
//! not when its client leaves.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::connectors::{MAX_ADMIN_STARTS, TOO_MANY_ADMIN_STARTS};
use mimorya_connectors::testing::{serve_demo_docs_http, serve_held_mcp_http};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, add, mcp_body};

const TRIAL: &str = "/admin/connectors/trial";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_has_two_administration_starts_under_way_at_most() {
    assert_eq!(MAX_ADMIN_STARTS, 2);
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let (held, hold) = serve_held_mcp_http().await;
    let answering = serve_demo_docs_http(None).await;
    let saved = add(&paul, mcp_body("Limite", "limite", &answering, None)).await;
    let trial = |url: &str| json!({ "prefix": "docs", "url": url });

    // Two trials reach a server that holds them.
    let mut first = Box::pin(paul.post(TRIAL, trial(&held)));
    let second = paul.post(TRIAL, trial(&held));
    tokio::pin!(second);
    let reached = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::select! {
            _ = &mut first => panic!("answered while held"),
            () = hold.reached() => {}
        }
        tokio::select! {
            _ = &mut first => panic!("answered while held"),
            _ = &mut second => panic!("answered while held"),
            () = hold.reached() => {}
        }
    });
    assert!(reached.await.is_ok(), "the server was never reached twice");
    // The first one's client leaves: its trial goes on, and keeps its slot.
    drop(first);

    // A third start, a trial or a test, is refused meanwhile.
    for refused in [
        paul.post(TRIAL, trial(&answering)).await,
        paul.post(&format!("/admin/connectors/{saved}/test"), json!({}))
            .await,
    ] {
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
        assert_eq!(refused.error_message(), TOO_MANY_ADMIN_STARTS);
    }

    // Once they end, their slots are free again.
    hold.release();
    let answered = second.await;
    assert_eq!(answered.status, StatusCode::OK, "{}", answered.text());
    api.idle().await;
    let tested = paul
        .post(&format!("/admin/connectors/{saved}/test"), json!({}))
        .await;
    assert_eq!(tested.status, StatusCode::OK, "{}", tested.text());
    assert_eq!(tested.json()["ok"], true);

    let deleted = paul.delete(&format!("/admin/connectors/{saved}")).await;
    assert!(deleted.status.is_success(), "{}", deleted.text());
}
