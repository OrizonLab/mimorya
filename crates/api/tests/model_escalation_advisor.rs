//! Anthropic's advisor (ADR-207, step 3), as a company chooses it and its
//! runs consult it. It waits for the operator's consent
//! (`MIMORYA_ANTHROPIC_ADVISOR`). Then it is offered on Anthropic at its
//! official address, the company's own key or the operator's, never past
//! an endpoint of the operator's choosing. It is kept for a pair of models
//! Anthropic accepts, and said not to serve once the main model changes to
//! one it does not advise. A run gives it to its main model, and each
//! consultation is recorded as the advisor's, counted in the month's
//! escalation spending.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::AdvisorTool;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::llm::{ENDPOINT, ESCALATION_MODEL, MASTER_KEY, OPERATOR_KEY};
use support::model_escalation::{ESCALATION, MAIN, escalation, own_provider, refused};
use support::refusals::{entries_since, latest_entry};
use support::runs::start_run;
use support::usage_roles::recorded_roles;
use support::{Client, TestApi};

const CONSENT: (&str, &str) = ("MIMORYA_ANTHROPIC_ADVISOR", "true");
const KEPT: [(&str, &str); 2] = [
    ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ("MIMORYA_TENANT_LLM_ENDPOINTS", ENDPOINT),
];

/// `ESCALATION_MODEL` as the advisor alone.
fn advising() -> Value {
    let mut chosen = escalation(ESCALATION_MODEL);
    chosen["advisor"] = json!(true);
    chosen
}

/// The advisor's line among the mechanisms.
fn advisor_offered(view: &Value) -> (bool, Option<String>) {
    let mechanism = view["mechanisms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|mechanism| mechanism["key"] == "advisor")
        .unwrap();
    let reason = mechanism["reason"].as_str().map(str::to_string);
    (mechanism["available"].as_bool().unwrap(), reason)
}

async fn leave(paul: &Client<'_>) {
    paul.delete(ESCALATION).await;
    paul.delete("/admin/llm").await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_advisor_waits_for_the_operator_s_consent() {
    let api = TestApi::start_with(&KEPT).await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    let (available, reason) = advisor_offered(&paul.get(ESCALATION).await.json());
    assert!(!available);
    assert!(reason.unwrap().contains("n’a pas ouvert"));
    refused(&paul, advising(), "n’a pas ouvert").await;
    // The other mechanisms do not wait for it.
    let mut after_failure = escalation(ESCALATION_MODEL);
    after_failure["afterFailure"] = json!(true);
    assert_eq!(
        paul.put(ESCALATION, after_failure).await.status,
        StatusCode::OK
    );
    leave(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn with_the_operator_s_consent_an_advisor_anthropic_accepts_is_kept() {
    let api = TestApi::start_with(&[KEPT[0], KEPT[1], CONSENT]).await;
    let paul = api.sign_in(user::PAUL).await;
    own_provider(&paul, MAIN).await;
    assert_eq!(
        advisor_offered(&paul.get(ESCALATION).await.json()),
        (true, None)
    );
    let since = latest_entry(&api).await;
    let kept = paul.put(ESCALATION, advising()).await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.text());
    let view = paul.get(ESCALATION).await.json();
    assert_eq!(view["settings"]["advisor"], true);
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("\"advisor\": true") || audit.contains("\"advisor\":true"));

    // The main model changed to one Anthropic does not let it advise: the
    // rest serves, the advisor is said not to.
    own_provider(&paul, "claude-sonnet-5-5").await;
    let view = paul.get(ESCALATION).await.json();
    assert_eq!(view["problem"], Value::Null);
    let (available, reason) = advisor_offered(&view);
    assert!(available);
    assert!(reason.unwrap().contains("ne peut pas conseiller"));
    refused(&paul, advising(), "ne peut pas conseiller").await;
    leave(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn on_the_operator_s_key_it_is_offered_at_anthropic_s_official_address_alone() {
    for (endpoint, offered) in [(None, true), (Some("https://passerelle.exemple/v1"), false)] {
        let mut env = vec![
            ("LLM_PROVIDER", "anthropic"),
            ("ANTHROPIC_API_KEY", OPERATOR_KEY),
            CONSENT,
        ];
        env.extend(endpoint.map(|url| ("LLM_BASE_URL", url)));
        let api = TestApi::start_with(&env).await;
        let paul = api.sign_in(user::PAUL).await;
        let view = paul.get(ESCALATION).await.json();
        assert_eq!(view["onOperatorKey"], true);
        let (available, reason) = advisor_offered(&view);
        assert_eq!(available, offered, "{endpoint:?}");
        if !offered {
            assert!(reason.unwrap().contains("adresse officielle"));
            refused(&paul, advising(), "adresse officielle").await;
        }
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_s_consultations_count_in_the_month_s_escalation_spending() {
    let api = TestApi::start_priced_with(&[KEPT[0], KEPT[1], CONSENT]).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    own_provider(&paul, MAIN).await;
    let kept = paul.put(ESCALATION, advising()).await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.text());
    let spent = |view: Value| view["spentMicros"].as_i64().unwrap();
    let before = spent(paul.get(ESCALATION).await.json());

    let answer = ScriptedReply::text("Trois interventions cette semaine.");
    api.company_model
        .push([ScriptedStep::Fixed(answer.consulting(ESCALATION_MODEL))]);
    let run = start_run(&marc).await;
    api.idle().await;

    // Given to the main model, as the company chose it; its escalation
    // model never called itself.
    let asked = api.company_model.requests().last().unwrap().advisor.clone();
    let given = AdvisorTool {
        model: ESCALATION_MODEL.into(),
        max_uses: 5,
        cache: true,
    };
    assert_eq!(asked, Some(given));
    assert!(api.escalation_model.requests().is_empty());
    assert_eq!(recorded_roles(&api, run).await, ["main", "advisor"]);
    // 1,000 tokens read and 400 written at the escalation model's prices.
    let consulted: (String, i64) = {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        sqlx::query_as(
            "select model, cost_micros from usage_records where run_id = $1 and role = 'advisor'",
        )
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
    };
    assert_eq!(consulted, (ESCALATION_MODEL.to_string(), 2_500_000));
    assert_eq!(spent(paul.get(ESCALATION).await.json()) - before, 2_500_000);
    leave(&paul).await;
}
