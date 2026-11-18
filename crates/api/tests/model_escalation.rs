//! The company's model escalation (ADR-207): a model of its runs'
//! provider, chosen by an administrator of its settings on the whole
//! company only, within a monthly budget and a number of escalations per
//! run; audited, removed. One chosen stops serving once the conversations'
//! provider changes. (On the platform's model: `model_supply_escalation.rs`;
//! what the mechanisms spend: `model_escalation_spending.rs`.)

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin};
use support::llm::{API_KEY, ENDPOINT};
use support::model_escalation::{
    ESCALATION, MAIN, STRONGER, escalation, own_provider, refused, start,
};
use support::refusals::{entries_since, latest_entry};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_chooses_its_escalation_model_within_its_bounds() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    let since = latest_entry(&api).await;
    let empty = paul.get(ESCALATION).await.json();
    assert_eq!(empty["mainModel"], MAIN);
    assert_eq!(empty["settings"], Value::Null);
    assert!(empty["spentMicros"].is_i64(), "{empty}");
    assert_eq!(empty["minBudgetMicros"], 1_000_000);
    assert_eq!(empty["maxBudgetMicros"], 10_000_000_000_i64);
    assert_eq!(empty["maxPerRunCeiling"], 50);
    // Escalation after a failed review (step 1) and the router (step 2)
    // are delivered; the advisor is not yet.
    let mechanisms: Vec<(&str, bool, &Value)> = empty["mechanisms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["key"].as_str().unwrap(),
                m["available"] == true,
                &m["reason"],
            )
        })
        .collect();
    let soon = json!("Bientôt disponible");
    assert_eq!(
        mechanisms,
        [
            ("afterFailure", true, &Value::Null),
            ("router", true, &Value::Null),
            ("advisor", false, &soon),
        ]
    );

    let mut cheap = escalation(STRONGER);
    cheap["monthlyBudgetMicros"] = json!(999_999);
    for (body, said) in [
        (escalation(MAIN), "doit différer du modèle principal"),
        (escalation(""), "Indiquez le modèle d’escalade"),
        (escalation("claude opus"), "Nom de modèle invalide"),
        (escalation("claude-sans-prix"), "ne connaît pas le prix"),
        (cheap, "1 à 10 000 USD"),
    ] {
        refused(&paul, body, said).await;
    }
    let mut too_many = escalation(STRONGER);
    too_many["maxPerRun"] = json!(51);
    refused(&paul, too_many, "1 à 50").await;
    let mut early = escalation(STRONGER);
    early["advisor"] = json!(true);
    refused(&paul, early, "pas encore disponible").await;
    let mut unknown = escalation(STRONGER);
    unknown["provider"] = json!("openai");
    let unknown = paul.put(ESCALATION, unknown).await;
    assert_eq!(
        unknown.status,
        StatusCode::BAD_REQUEST,
        "{}",
        unknown.text()
    );

    let chosen = paul.put(ESCALATION, escalation(STRONGER)).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
    let shown = paul.get(ESCALATION).await.json();
    let settings = &shown["settings"];
    assert_eq!(settings["model"], STRONGER);
    assert_eq!(settings["monthlyBudgetMicros"], 50_000_000);
    assert_eq!(settings["maxPerRun"], 5);
    assert_eq!(settings["afterFailure"], false);
    assert_eq!(shown["problem"], Value::Null);
    let mut cheaper = escalation("claude-sonnet-5-5");
    cheaper["maxPerRun"] = json!(50);
    assert_eq!(paul.put(ESCALATION, cheaper).await.status, StatusCode::OK);
    assert_eq!(
        paul.get(ESCALATION).await.json()["settings"]["maxPerRun"],
        50
    );

    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
    assert_eq!(paul.get(ESCALATION).await.json()["settings"], Value::Null);
    let again = paul.delete(ESCALATION).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    assert!(
        again.error_message().contains("Aucune escalade"),
        "{}",
        again.text()
    );
    let audit = entries_since(&api, since).await;
    for said in [
        "model_escalation.configured",
        "\"model\":\"claude-opus-5-5\"",
        "\"previousModel\":\"claude-opus-5-5\"",
        "\"monthlyBudgetMicros\":50000000",
        "model_escalation.reset",
    ] {
        assert!(audit.contains(said), "{said}: {audit}");
    }
    assert!(!audit.contains(API_KEY), "{audit}");
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_escalation_stops_serving_once_the_conversations_provider_changes() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    assert_eq!(
        paul.put(ESCALATION, escalation(STRONGER)).await.status,
        StatusCode::OK
    );
    // The same model for both: nothing to escalate to.
    own_provider(&paul, STRONGER).await;
    let shown = paul.get(ESCALATION).await.json();
    assert!(
        shown["problem"].as_str().unwrap().contains("doit différer"),
        "{shown}"
    );
    // Another provider: the choice is kept, but no longer serves.
    let local = json!({ "provider": "ollama", "model": "llama3.3", "baseUrl": ENDPOINT });
    assert_eq!(paul.put("/admin/llm", local).await.status, StatusCode::OK);
    let shown = paul.get(ESCALATION).await.json();
    assert_eq!(shown["settings"]["model"], STRONGER);
    assert!(
        shown["problem"].as_str().unwrap().contains("a changé"),
        "{shown}"
    );
    // Back on the platform's model.
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
    let shown = paul.get(ESCALATION).await.json();
    assert!(
        shown["problem"]
            .as_str()
            .unwrap()
            .contains("fournisseur propre"),
        "{shown}"
    );
    assert_eq!(paul.delete(ESCALATION).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_administrators_of_its_settings_see_or_set_it() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.escalade").await;
    let partial = activate(&api, &mut partial).await;
    for client in [&lucas, &partial] {
        assert_eq!(client.get(ESCALATION).await.status, StatusCode::FORBIDDEN);
        let put = client.put(ESCALATION, escalation(STRONGER)).await;
        assert_eq!(put.status, StatusCode::FORBIDDEN);
        assert_eq!(
            client.delete(ESCALATION).await.status,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(paul.get(ESCALATION).await.json()["settings"], Value::Null);
    assert_eq!(paul.delete("/admin/llm").await.status, StatusCode::OK);
}
