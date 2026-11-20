//! The company's decision service as its administrators set it (ADR-213):
//! its main model by default; Jev only where the operator opens it, with
//! the company's key sealed, never shown, journaled nor audited; usages off
//! until their step is delivered, thresholds from 0 to 1; reserved to the
//! administrators of its settings on the whole company; another company's
//! settings out of reach. (Its engines asked: `decision_engines.rs`.)

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeJev, JEV_KEY};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::companies;
use support::decisions::{DECISIONS, USAGES, refused, set, settings};
use support::mcp::capture_logs;
use support::refusals::{entries_since, latest_entry};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_decides_with_its_main_model_until_the_operator_opens_jev() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, false).await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let shown = paul.get(DECISIONS).await.json();
    assert_eq!(shown["engine"], "mainModel", "{shown}");
    assert_eq!(shown["jevAvailable"], false);
    assert!(
        shown["jevReason"]
            .as_str()
            .unwrap()
            .contains("n’a pas ouvert")
    );
    assert_eq!(shown["jevModel"], "jev-1.14.0");
    assert_eq!(shown["hasJevKey"], false);
    assert_eq!(shown["problem"], Value::Null);
    assert_eq!(shown["updatedAt"], Value::Null);
    let usages = shown["usages"].as_array().unwrap();
    assert_eq!(usages.len(), USAGES.len());
    for (usage, key) in usages.iter().zip(USAGES) {
        assert_eq!(usage["key"], key);
        // Each off, at its default threshold; only the model's routing, the
        // skill choice and the memory guard are delivered (D1 to D3).
        let delivered = matches!(key, "modelRouting" | "skillChoice" | "memoryGuard");
        assert_eq!(
            (&usage["available"], &usage["mode"], &usage["threshold"]),
            (&json!(delivered), &json!("off"), &json!(0.5)),
            "{usage}"
        );
    }

    // Jev and its key refused without the operator's consent.
    refused(
        &paul,
        settings("jev", Some(JEV_KEY)),
        StatusCode::BAD_REQUEST,
        "n’a pas ouvert",
    )
    .await;
    refused(
        &paul,
        settings("mainModel", Some(JEV_KEY)),
        StatusCode::BAD_REQUEST,
        "n’a pas ouvert",
    )
    .await;
    // A usage not delivered stays off; a threshold stays within 0 and 1;
    // each usage is set once; nothing else is read.
    let mut early = settings("mainModel", None);
    early["usages"][3]["mode"] = json!("observe");
    refused(
        &paul,
        early,
        StatusCode::BAD_REQUEST,
        "pas encore disponible",
    )
    .await;
    for threshold in [json!(1.5), json!(-0.1)] {
        let mut out = settings("mainModel", None);
        out["usages"][2]["threshold"] = threshold;
        refused(&paul, out, StatusCode::BAD_REQUEST, "va de 0 à 1").await;
    }
    let mut twice = settings("mainModel", None);
    twice["usages"][1]["key"] = json!("modelRouting");
    refused(&paul, twice, StatusCode::BAD_REQUEST, "réglé deux fois").await;
    let mut unknown = settings("mainModel", None);
    unknown["anthropic"] = json!(true);
    let unknown = paul.put(DECISIONS, unknown).await;
    assert_eq!(
        unknown.status,
        StatusCode::BAD_REQUEST,
        "{}",
        unknown.text()
    );

    let mut tuned = settings("mainModel", None);
    tuned["usages"][3]["threshold"] = json!(0.8);
    set(&paul, tuned).await;
    let shown = paul.get(DECISIONS).await.json();
    assert_eq!(shown["engine"], "mainModel");
    assert_eq!(shown["usages"][3]["threshold"], 0.8);
    assert!(shown["updatedAt"].is_string(), "{shown}");
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
    assert_eq!(paul.get(DECISIONS).await.json()["updatedAt"], Value::Null);
    let again = paul.delete(DECISIONS).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND, "{}", again.text());
    let audit = entries_since(&api, since).await;
    for said in [
        "decision_service.configured",
        "\"engine\":\"main_model\"",
        "\"threshold\":0.8",
        "decision_service.reset",
    ] {
        assert!(audit.contains(said), "{said}: {audit}");
    }
    assert!(jev.asked().is_empty(), "Jev was asked: {:?}", jev.asked());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_key_to_typesafe_is_sealed_and_never_shown_logged_nor_audited() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    let logs = capture_logs();
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let shown = paul.get(DECISIONS).await.json();
    assert_eq!(
        (&shown["jevAvailable"], &shown["jevReason"]),
        (&json!(true), &Value::Null)
    );

    // Jev needs a key, and one that reads as a key.
    refused(
        &paul,
        settings("jev", None),
        StatusCode::BAD_REQUEST,
        "clé d’API TypeSafe",
    )
    .await;
    refused(
        &paul,
        settings("jev", Some("cle avec espaces")),
        StatusCode::BAD_REQUEST,
        "Clé d’API invalide",
    )
    .await;
    set(&paul, settings("jev", Some(JEV_KEY))).await;
    let shown = paul.get(DECISIONS).await;
    assert_eq!(shown.json()["engine"], "jev");
    assert_eq!(shown.json()["hasJevKey"], true);
    assert_eq!(shown.json()["problem"], Value::Null);
    // Kept when no other is sent; the main model chosen again keeps it too.
    set(&paul, settings("jev", None)).await;
    set(&paul, settings("mainModel", None)).await;
    assert_eq!(paul.get(DECISIONS).await.json()["hasJevKey"], true);

    let audit = entries_since(&api, since).await;
    assert!(audit.contains("\"keyReplaced\":true"), "{audit}");
    assert!(audit.contains("\"engine\":\"jev\""), "{audit}");
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let stored: Vec<Vec<u8>> = sqlx::query_scalar("select ciphertext from secrets")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for (seen, said) in [
        (shown.text(), "its settings"),
        (audit, "the audit"),
        (logs.text(), "the logs"),
        (format!("{stored:?}"), "the secrets"),
    ] {
        assert!(!seen.contains(JEV_KEY), "the key in {said}: {seen}");
    }
    assert!(
        stored
            .iter()
            .all(|sealed| !String::from_utf8_lossy(sealed).contains(JEV_KEY)),
        "the key stored in clear"
    );

    // Removed with the settings.
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
    let shown = paul.get(DECISIONS).await.json();
    assert_eq!(
        (&shown["engine"], &shown["hasJevKey"]),
        (&json!("mainModel"), &json!(false))
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_administrators_of_its_settings_see_or_set_it() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.decisions").await;
    let partial = activate(&api, &mut partial).await;
    for client in [&lucas, &partial] {
        assert_eq!(client.get(DECISIONS).await.status, StatusCode::FORBIDDEN);
        let put = client.put(DECISIONS, settings("jev", Some(JEV_KEY))).await;
        assert_eq!(put.status, StatusCode::FORBIDDEN);
        assert_eq!(client.delete(DECISIONS).await.status, StatusCode::FORBIDDEN);
        let test = client.post(&format!("{DECISIONS}/test"), json!({})).await;
        assert_eq!(test.status, StatusCode::FORBIDDEN);
    }
    assert_eq!(paul.get(DECISIONS).await.json()["updatedAt"], Value::Null);
    assert!(jev.asked().is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_s_settings_stay_out_of_another_s_reach() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    let paul = api.sign_in(user::PAUL).await;
    set(&paul, settings("jev", Some(JEV_KEY))).await;
    let other = companies::create(&api, "acme-decisions", "jeanne@acme-decisions.example").await;
    let db = &api.app.services.db;
    let mut tx = db.begin(other.tenant_id).await.unwrap();
    // Néo Maintenance's rows are not seen from the other company…
    let seen: i64 = sqlx::query_scalar(
        "select (select count(*) from tenant_decision_settings) \
         + (select count(*) from tenant_decision_usages)",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(seen, 0);
    // …nor changed, nor written in its name.
    let changed = sqlx::query("update tenant_decision_settings set engine = 'main_model'")
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(changed, 0);
    let written = sqlx::query(
        "insert into tenant_decision_usages (tenant_id, usage, mode, threshold) \
         values ($1, 'action_guard', 'active', 0)",
    )
    .bind(tenant::ID)
    .execute(&mut *tx)
    .await;
    assert!(written.is_err(), "written in another company's name");
    drop(tx);
    assert_eq!(paul.get(DECISIONS).await.json()["engine"], "jev");
    assert_eq!(paul.delete(DECISIONS).await.status, StatusCode::OK);
}
