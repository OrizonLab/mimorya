//! What the operator's supply holds back besides runs (ADR-208, step 1):
//! past the company's monthly cap, no lesson is drawn from a run nor a
//! conversation's summary written, and a skill's evaluation makes no more
//! call, its judge's included: it ends saying why, and the next one is
//! refused. Each time, the call that went past the cap was the last made.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::provisioning::CapChoice;
use mimorya_core::demo::user;
use mimorya_core::model_supply::{SupplyFormula, SupplyStop};
use mimorya_db::seed::DEMO_SKILL_ID;
use sqlx::PgPool;
use support::lessons::{capture_lessons, lessons, run_with_tools};
use support::llm::SCRIPTED_CALL_MICROS;
use support::model_supply::{CAP, capped, spent, supply};
use support::refusals::{entries_since, latest_entry};
use support::skill_evaluations::{
    forget, launch, launched, new_version, play, played, status_of, write_case,
};
use support::skill_proposals::restore_the_report;
use support::summaries::{SUMMARY, conversation, exchanges, stored, turn};
use support::{TestApi, owner_url};

/// The company on a 1 USD cap, with room left for `calls` scripted calls
/// and half of one more: the last of them goes past the cap.
async fn room_for(owner: &PgPool, calls: i64) {
    supply(owner, capped(CapChoice::Amount(CAP))).await;
    let room = calls * SCRIPTED_CALL_MICROS - SCRIPTED_CALL_MICROS / 2;
    spent(owner, SupplyFormula::Capped, CAP - room).await;
}

async fn leave(owner: &PgPool) {
    spent(owner, SupplyFormula::Capped, 0).await;
    supply(owner, capped(CapChoice::Default)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn past_the_cap_no_lesson_is_drawn_from_a_run() {
    let api = TestApi::start_priced().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    capture_lessons(&paul, true).await;
    // The search, then the answer, which goes past the cap.
    room_for(&owner, 2).await;

    run_with_tools(&api, &marc, Vec::new()).await;
    assert_eq!(api.model.as_ref().unwrap().requests().len(), 2);
    assert!(lessons(&api).await.is_empty());
    capture_lessons(&paul, false).await;
    leave(&owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn past_the_cap_no_summary_is_written() {
    let api = TestApi::start_priced().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 8).await;
    // The answer goes past the cap: eighteen messages, nothing folded.
    room_for(&owner, 1).await;

    let steps = vec![ScriptedStep::text("Réponse 9"), ScriptedStep::text(SUMMARY)];
    turn(&api, &marc, c, "Question 9", steps).await;
    assert_eq!(api.model.as_ref().unwrap().requests().len(), 1);
    assert_eq!(api.model.as_ref().unwrap().remaining(), 1);
    assert_eq!(stored(c).await, None);
    leave(&owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn past_the_cap_an_evaluation_makes_no_more_call_and_says_why() {
    let api = TestApi::start_priced().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let paul = api.sign_in(user::PAUL).await;
    let version = new_version(&paul).await;
    write_case(
        &paul,
        DEMO_SKILL_ID,
        "Retards",
        "Quels retards cette semaine ?",
    )
    .await;
    // The agent's answer to the first case goes past the cap.
    room_for(&owner, 1).await;
    let mut script = played(false, "Trois retards cette semaine.", true);
    script.extend(played(false, "Trois retards cette semaine.", true));
    api.script(script);

    let since = latest_entry(&api).await;
    let id = launched(&paul, DEMO_SKILL_ID, version).await;
    play(&api).await;
    // Neither judged, nor the case played for the other version.
    assert_eq!(api.model.as_ref().unwrap().requests().len(), 1);
    let (status, failure, _) = status_of(&api, &id).await;
    let told = SupplyStop::MonthlyCap.message();
    assert_eq!(
        (status.as_str(), failure.as_deref()),
        ("failed", Some(told))
    );
    // Nor is another launched.
    let refused = launch(&paul, DEMO_SKILL_ID, version).await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(refused.error_message(), told);
    // The evaluation's end traced, and the refusal.
    let audit = entries_since(&api, since).await;
    assert_eq!(audit.matches("model_supply.limited").count(), 2, "{audit}");
    forget(&api, DEMO_SKILL_ID).await;
    restore_the_report(&api).await;
    leave(&owner).await;
}
