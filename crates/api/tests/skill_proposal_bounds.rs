//! What bounds the skills the agent proposes (ADR-100): too many proposals
//! waiting, in the company or for one person, refuse one more; and when
//! the company has managers validate the actions of their risk, the
//! person's manager reads the exact text and alone decides.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use mimorya_core::skill_proposals::{MAX_PENDING_PER_PERSON, MAX_PENDING_PROPOSALS};
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::json;
use support::TestApi;
use support::manager_approvals::{listed, route_to_managers};
use support::skill_proposals::{
    RATIONALE, detail, name_of, proposed, read_proposed, read_refused, restore_the_report,
    validate, version_of_the_report, waiting,
};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn too_many_proposals_waiting_refuse_one_more() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let paul = api.sign_in(user::PAUL).await;
    let (run, approval) = proposed(&api, &lucas, version_of_the_report(), read_proposed()).await;
    validate(&api, &lucas, &approval).await;

    // As many waiting for Lucas as one person may have: no more for him.
    waiting(&api, MAX_PENDING_PER_PERSON - 1, user::LUCAS, run).await;
    let (_, approval) = proposed(
        &api,
        &lucas,
        version_of_the_report(),
        read_refused("pour cette personne"),
    )
    .await;
    validate(&api, &lucas, &approval).await;
    restore_the_report(&api).await;

    // As many waiting in the company as it takes, others': no more.
    waiting(&api, MAX_PENDING_PROPOSALS, user::MARC, run).await;
    let (_, approval) = proposed(
        &api,
        &lucas,
        version_of_the_report(),
        read_refused("attendent déjà les administrateurs"),
    )
    .await;
    validate(&api, &lucas, &approval).await;
    let shown = detail(&paul, &DEMO_SKILL_ID.to_string()).await;
    assert_eq!(shown["skill"]["proposals"], MAX_PENDING_PROPOSALS);
    restore_the_report(&api).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_manager_validates_a_proposal_when_the_company_says_so() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    route_to_managers(&paul, &["draft"]).await;

    let (run, approval) = proposed(&api, &nadia, version_of_the_report(), read_proposed()).await;
    // Nadia's manager reads the exact text, and alone decides.
    let request = listed(&marc, &run.to_string())
        .await
        .expect("listed to Marc");
    assert_eq!(request["canDecide"], true);
    assert_eq!(request["requestedFor"]["id"], json!(user::NADIA));
    assert_eq!(request["payload"]["rationale"], RATIONALE);
    let decision = format!("/approvals/{approval}/decision");
    let hers = nadia.post(&decision, json!({ "approved": true })).await;
    assert_eq!(hers.status, StatusCode::FORBIDDEN, "{}", hers.text());
    validate(&api, &marc, &approval).await;

    let shown = detail(&paul, &DEMO_SKILL_ID.to_string()).await;
    assert_eq!(shown["versions"][0]["status"], "candidate");
    let nadia_name = name_of(&api, user::NADIA).await;
    assert_eq!(shown["versions"][0]["createdBy"], nadia_name.as_str());
    route_to_managers(&paul, &[]).await;
    restore_the_report(&api).await;
}
