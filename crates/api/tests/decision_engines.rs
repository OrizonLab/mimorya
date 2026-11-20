//! The company's decision engines asked (ADR-213): Jev on its own key,
//! counted with the role « décision » on that key; the main model charged
//! as the company's other calls, never past its supply; Jev failing — an
//! overload, a silence, an unreadable answer — falls back to the main model,
//! and the test of the engine chosen says it failed without falling back;
//! an answer off its question is counted and never decides; the decision
//! journaled, never the state it was taken on.

mod support;

use axum::http::StatusCode;
use mimorya_agent::decision::{
    DecisionOutcome, DecisionQuestion, DecisionRequest, DecisionState, EngineKind,
};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::llm::DecisionScope;
use mimorya_api::services::provisioning::{CapChoice, SupplyChange};
use mimorya_connectors::testing::{FakeJev, JEV_KEY, JEV_YES, JevBehaviour};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::SupplyFormula;
use serde_json::json;
use sqlx::PgPool;
use support::decisions::{
    DECISIONS, decision_calls, forget_decision_calls, main_model_yes, set, settings,
};
use support::llm::SCRIPTED_CALL_MICROS;
use support::mcp::capture_logs;
use support::model_supply::{CAP, capped, spent, supply};
use support::refusals::{entries_since, latest_entry};
use support::{TestApi, owner_url};

const TEST: &str = "/admin/decisions/test";
/// What the state of the engine's test says.
const TEST_STATE: &str = "comment allez-vous";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn jev_decides_on_the_company_s_key_and_is_counted_on_it() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    forget_decision_calls().await;
    let logs = capture_logs();
    let paul = api.sign_in(user::PAUL).await;
    set(&paul, settings("jev", Some(JEV_KEY))).await;
    let since = latest_entry(&api).await;
    let tested = paul.post(TEST, json!({})).await;
    assert_eq!(tested.status, StatusCode::OK, "{}", tested.text());
    let tested = tested.json();
    assert_eq!(tested["engine"], "jev");
    assert_eq!(tested["model"], "jev-1.14.0");
    assert_eq!(tested["yes"], JEV_YES);
    // The pinned version asked, on the key alone, the state the test's only.
    let asked = jev.asked();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].keyed);
    assert_eq!(asked[0].body["model"], "jev-1.14.0");
    assert_eq!(
        asked[0].body["state"],
        json!({ "message": "Bonjour, comment allez-vous ?" })
    );
    assert_eq!(asked[0].body["questions"]["salutation"]["type"], "noul");
    // Counted on the company's own key: never against the operator's supply.
    let calls = decision_calls(&api).await;
    assert_eq!(calls.len(), 1, "{calls:?}");
    let (provider, model, formula, _, billed) = &calls[0];
    assert_eq!(
        (provider.as_str(), model.as_str(), formula.as_str(), billed),
        ("typesafe", "jev-1.14.0", "client_key", &None)
    );
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("decision_service.tested"), "{audit}");
    assert!(audit.contains("\"ok\":true"), "{audit}");
    // The decision journaled, never the state it was taken on.
    let journaled = logs.text();
    assert!(journaled.contains("engine_test"), "{journaled}");
    assert!(journaled.contains("decided"), "{journaled}");
    assert!(!journaled.contains(TEST_STATE), "{journaled}");
    assert!(!audit.contains(TEST_STATE), "{audit}");
    // Nor the key it was sent.
    assert!(!journaled.contains(JEV_KEY), "{journaled}");
    assert!(!audit.contains(JEV_KEY), "{audit}");
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_test_of_a_failing_jev_says_so_without_falling_back() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    forget_decision_calls().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    set(&paul, settings("jev", Some("ts-une-autre-cle-0123456789"))).await;
    let refused = paul.post(TEST, json!({})).await;
    assert_eq!(
        refused.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        refused.text()
    );
    assert!(
        refused.error_message().contains("n’a pas répondu"),
        "{}",
        refused.text()
    );
    set(&paul, settings("jev", Some(JEV_KEY))).await;
    for behaviour in [
        JevBehaviour::Overloaded,
        JevBehaviour::Unreadable,
        JevBehaviour::Silent,
    ] {
        jev.behave(behaviour);
        let failed = paul.post(TEST, json!({})).await;
        assert_eq!(
            failed.status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{behaviour:?}"
        );
    }
    // Nothing answered a decision: nothing counted, the main model never asked.
    assert!(decision_calls(&api).await.is_empty());
    assert!(api.model.as_ref().unwrap().requests().is_empty());
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("\"ok\":false"), "{audit}");
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
}

fn salutation() -> DecisionRequest {
    DecisionRequest {
        usage: "engine_test",
        state: DecisionState::new().field("message", "Bonjour", 100),
        questions: vec![DecisionQuestion::noul(
            "salutation",
            "Ce message est-il une salutation ?",
        )],
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn jev_failing_falls_back_to_the_main_model() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    forget_decision_calls().await;
    let paul = api.sign_in(user::PAUL).await;
    set(&paul, settings("jev", Some(JEV_KEY))).await;
    let llm = &api.app.services.llm;
    let service = llm
        .decision_service(tenant::ID, DecisionScope::default())
        .await
        .unwrap();
    assert_eq!(service.engines(), [EngineKind::Jev, EngineKind::MainModel]);
    for behaviour in [
        JevBehaviour::Overloaded,
        JevBehaviour::Unreadable,
        JevBehaviour::Silent,
    ] {
        jev.behave(behaviour);
        api.script(vec![ScriptedStep::text(main_model_yes("forte"))]);
        let DecisionOutcome::Decided(decided) = service.decide(&salutation()).await else {
            panic!("{behaviour:?}: no decision");
        };
        assert_eq!(decided.engine, EngineKind::MainModel, "{behaviour:?}");
        assert_eq!(decided.yes("salutation"), Some(0.95));
    }
    // Neither engine deciding fails the decision, which the usage takes as
    // its prudent value.
    jev.behave(JevBehaviour::Overloaded);
    api.script(vec![ScriptedStep::text("Oui, c’est une salutation.")]);
    assert!(matches!(
        service.decide(&salutation()).await,
        DecisionOutcome::Failed
    ));
    // Jev answered nothing: the main model's calls alone are counted.
    let providers: Vec<String> = decision_calls(&api)
        .await
        .into_iter()
        .map(|call| call.0)
        .collect();
    assert_eq!(providers, ["scripted"; 4]);
    // Back to the main model alone.
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
    let alone = llm
        .decision_service(tenant::ID, DecisionScope::default())
        .await
        .unwrap();
    assert_eq!(alone.engines(), [EngineKind::MainModel]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_main_model_decides_charged_as_the_company_s_other_calls() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, false).await;
    forget_decision_calls().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let paul = api.sign_in(user::PAUL).await;
    supply(&owner, capped(CapChoice::Default)).await;
    supply(&owner, SupplyChange::Markup { bps: 1_000 }).await;
    spent(&owner, SupplyFormula::Capped, 0).await;
    api.script(vec![ScriptedStep::text(main_model_yes("moyenne"))]);
    let tested = paul.post(TEST, json!({})).await;
    assert_eq!(tested.status, StatusCode::OK, "{}", tested.text());
    let tested = tested.json();
    assert_eq!(tested["engine"], "mainModel");
    assert_eq!(tested["model"], "scripted-model");
    assert_eq!(tested["yes"], 0.8);
    // Asked in strict JSON, the state framed as data it may not obey.
    let asked = api.model.as_ref().unwrap().requests();
    let asked = format!("{:?}", asked.last().unwrap());
    assert!(asked.contains("etat_de_decision"), "{asked}");
    // Charged on the operator's key, at the amount re-invoiced.
    let calls = decision_calls(&api).await;
    assert_eq!(
        calls,
        [(
            "scripted".to_string(),
            "scripted-model".to_string(),
            "capped".to_string(),
            SCRIPTED_CALL_MICROS,
            Some(SCRIPTED_CALL_MICROS + SCRIPTED_CALL_MICROS / 10),
        )]
    );

    // An answer off its question is counted, and decides nothing.
    api.script(vec![ScriptedStep::text(
        "{\"salutation\": {\"reponse\": \"peut-être\", \"certitude\": \"forte\"}}",
    )]);
    let failed = paul.post(TEST, json!({})).await;
    assert_eq!(
        failed.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        failed.text()
    );
    assert_eq!(decision_calls(&api).await.len(), 2);

    // Past its supply, the main model is not asked: the test says why, and
    // is traced as a run's start would be.
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    spent(&owner, SupplyFormula::Capped, CAP).await;
    let before = api.model.as_ref().unwrap().requests().len();
    let since = latest_entry(&api).await;
    let stopped = paul.post(TEST, json!({})).await;
    assert_eq!(
        stopped.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        stopped.text()
    );
    assert!(
        stopped.error_message().contains("plafond mensuel"),
        "{}",
        stopped.text()
    );
    assert_eq!(api.model.as_ref().unwrap().requests().len(), before);
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_supply.limited"), "{audit}");
    spent(&owner, SupplyFormula::Capped, 0).await;
    supply(&owner, capped(CapChoice::Default)).await;
    supply(&owner, SupplyChange::Markup { bps: 0 }).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn jev_closed_by_the_operator_after_a_company_chose_it_is_never_asked() {
    let jev = FakeJev::serve().await;
    let opened = TestApi::start_with_jev(&jev, true).await;
    forget_decision_calls().await;
    set(
        &opened.sign_in(user::PAUL).await,
        settings("jev", Some(JEV_KEY)),
    )
    .await;
    // The operator closes Jev: its DPA ended, say.
    let api = TestApi::start_with_jev(&jev, false).await;
    let paul = api.sign_in(user::PAUL).await;
    let shown = paul.get(DECISIONS).await.json();
    assert_eq!(
        (&shown["engine"], &shown["jevAvailable"]),
        (&json!("jev"), &json!(false))
    );
    assert!(
        shown["problem"]
            .as_str()
            .unwrap()
            .contains("n’a pas ouvert"),
        "{shown}"
    );
    // Decisions go to the main model alone…
    let service = api
        .app
        .services
        .llm
        .decision_service(tenant::ID, DecisionScope::default())
        .await
        .unwrap();
    assert_eq!(service.engines(), [EngineKind::MainModel]);
    api.script(vec![ScriptedStep::text(main_model_yes("forte"))]);
    let DecisionOutcome::Decided(decided) = service.decide(&salutation()).await else {
        panic!("no decision");
    };
    assert_eq!(decided.engine, EngineKind::MainModel);
    // …the engine chosen is not tested…
    let refused = paul.post(TEST, json!({})).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused.error_message().contains("n’a pas ouvert"),
        "{}",
        refused.text()
    );
    // …and Jev is never asked.
    assert!(jev.asked().is_empty(), "{:?}", jev.asked());
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn jev_s_price_is_shown_where_it_served_even_once_closed() {
    let jev = FakeJev::serve().await;
    let closed = TestApi::start_with_jev(&jev, false).await;
    forget_decision_calls().await;
    let usage = "/admin/usage?days=30";
    let shown = closed.sign_in(user::PAUL).await.get(usage).await.json();
    // Closed and never asked: no price of it.
    assert_eq!(shown["jevPrices"], json!([]), "{shown}");
    let opened = TestApi::start_with_jev(&jev, true).await;
    let paul = opened.sign_in(user::PAUL).await;
    // Opened: its dated price, with its source.
    let shown = paul.get(usage).await.json();
    let price = &shown["jevPrices"][0];
    assert_eq!(price["model"], "jev-1.14.0", "{shown}");
    assert_eq!(price["inputPerMtok"], 0.042);
    assert_eq!(price["outputPerMtok"], 0.0);
    assert_eq!(price["readOn"], "2026-11-19");
    assert_eq!(price["source"], "https://docs.typesafe.ai/models");
    set(&paul, settings("jev", Some(JEV_KEY))).await;
    assert_eq!(paul.post(TEST, json!({})).await.status, StatusCode::OK);
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
    // Closed again after it served: still shown beside its calls.
    let shown = closed.sign_in(user::PAUL).await.get(usage).await.json();
    assert_eq!(shown["jevPrices"][0]["model"], "jev-1.14.0", "{shown}");
}
