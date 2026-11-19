//! Anthropic's advisor on the operator's key (ADR-207, step 3, beside
//! ADR-208 to ADR-212): in the formulas 2 and 3, a company on the
//! platform's model consults its escalation model as its advisor, each
//! consultation recorded once on the operator's key, at the amount
//! re-invoiced. That amount counts in the escalation's budget and in the
//! cap or the credit, with the run's other calls; once it takes the month
//! past the cap, the run's next call is not made.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::AdvisorTool;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call};
use mimorya_api::services::provisioning::{CapChoice, SupplyChange, model_supply_report};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::{SupplyFormula, SupplyStop};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::llm::{ESCALATION_MODEL, OPERATOR_KEY, SCRIPTED_CALL_MICROS};
use support::model_escalation::{ESCALATION, MAIN, charged, escalation, loaded};
use support::model_supply::{CAP, capped, spent, supply};
use support::runs::start_run;
use support::{TestApi, owner_url};

/// 10 %: what a call is re-invoiced, beside its cost.
const MARKUP_BPS: u32 = 1_000;
const BILLED: i64 = SCRIPTED_CALL_MICROS + SCRIPTED_CALL_MICROS / 10;
/// A consultation (1,000 tokens read, 400 written) at the escalation
/// model's prices, and what it is re-invoiced.
const CONSULTED: i64 = 2_500_000;
const CONSULTED_BILLED: i64 = CONSULTED + CONSULTED / 10;

/// The platform's model, named as an Anthropic model the escalation model
/// may advise, on the operator's key, the operator opening the advisor.
async fn start() -> TestApi {
    TestApi::start_named_priced_with(
        MAIN,
        &[
            ("LLM_PROVIDER", "anthropic"),
            ("ANTHROPIC_API_KEY", OPERATOR_KEY),
            ("MIMORYA_ANTHROPIC_ADVISOR", "true"),
        ],
    )
    .await
}

fn formula(formula: SupplyFormula) -> SupplyChange {
    SupplyChange::Formula {
        formula,
        cap: CapChoice::Default,
    }
}

/// `ESCALATION_MODEL` as the advisor alone.
fn advising() -> Value {
    let mut chosen = escalation(ESCALATION_MODEL);
    chosen["advisor"] = json!(true);
    chosen
}

fn spent_micros(view: &Value) -> i64 {
    view["spentMicros"].as_i64().unwrap()
}

async fn leave(api: &TestApi, owner: &PgPool) {
    let paul = api.sign_in(user::PAUL).await;
    paul.delete(ESCALATION).await;
    supply(owner, SupplyChange::Markup { bps: 0 }).await;
    supply(owner, capped(CapChoice::Default)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn its_consultations_count_in_the_budget_and_in_the_supply_at_the_amount_paid() {
    let api = start().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    supply(&owner, SupplyChange::Markup { bps: MARKUP_BPS }).await;
    let paid = SupplyChange::Credit {
        amount_micros: 100_000_000,
        reference: "FAC-2026-1120".into(),
    };
    supply(&owner, paid).await;
    let model = api.model.clone().unwrap();

    for chosen in [SupplyFormula::Capped, SupplyFormula::Prepaid] {
        supply(&owner, formula(chosen)).await;
        let shown = paul.get(ESCALATION).await.json();
        assert_eq!(
            (&shown["mainModel"], &shown["onOperatorKey"]),
            (&json!(MAIN), &json!(true)),
            "{shown}"
        );
        let set = paul.put(ESCALATION, advising()).await;
        assert_eq!(set.status, StatusCode::OK, "{}", set.text());
        loaded(&api, chosen, true).await;
        let budget_before = spent_micros(&paul.get(ESCALATION).await.json());
        let before = model_supply_report(&owner, tenant::SLUG).await.unwrap();

        let answer = ScriptedReply::text("Trois interventions cette semaine.");
        model.push([ScriptedStep::Fixed(answer.consulting(ESCALATION_MODEL))]);
        let run = start_run(&marc).await;
        api.idle().await;
        // Given to the platform's model, never called itself.
        let given = AdvisorTool {
            model: ESCALATION_MODEL.into(),
            max_uses: 5,
            cache: true,
        };
        assert_eq!(model.requests().last().unwrap().advisor, Some(given));
        assert!(api.escalation_model.requests().is_empty());
        // Each recorded once, on the operator's key, at its own prices.
        let formula = chosen.as_str().to_string();
        assert_eq!(
            charged(&api, run).await,
            [
                (
                    "main".into(),
                    formula.clone(),
                    SCRIPTED_CALL_MICROS,
                    Some(BILLED)
                ),
                ("advisor".into(), formula, CONSULTED, Some(CONSULTED_BILLED)),
            ],
            "{chosen:?}"
        );
        // In the budget, at what the company pays for it…
        let budget_after = spent_micros(&paul.get(ESCALATION).await.json());
        assert_eq!(budget_after - budget_before, CONSULTED_BILLED, "{chosen:?}");
        // …and in the cap or the credit, with the run's own call.
        let after = model_supply_report(&owner, tenant::SLUG).await.unwrap();
        let counted = match chosen {
            SupplyFormula::Capped => {
                after.spending.capped_billed_micros - before.spending.capped_billed_micros
            }
            _ => before.spending.balance_micros() - after.spending.balance_micros(),
        };
        assert_eq!(counted, BILLED + CONSULTED_BILLED, "{chosen:?}");
    }
    leave(&api, &owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_consultation_that_takes_the_month_past_the_cap_stops_the_run_s_next_call() {
    let api = start().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    supply(&owner, capped(CapChoice::Amount(CAP))).await;
    let set = paul.put(ESCALATION, advising()).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    loaded(&api, SupplyFormula::Capped, true).await;
    // Room for both of the main model's calls, not for a consultation.
    spent(
        &owner,
        SupplyFormula::Capped,
        CAP - 3 * SCRIPTED_CALL_MICROS,
    )
    .await;

    let model = api.model.clone().unwrap();
    let searched = ScriptedReply::calls("", vec![call("interventions_search", json!({}))]);
    model.push([
        ScriptedStep::Fixed(searched.consulting(ESCALATION_MODEL)),
        ScriptedStep::text("Trois interventions cette semaine."),
    ]);
    let run = start_run(&marc).await;
    api.idle().await;
    let read = marc.get(&format!("/runs/{run}")).await.json()["run"].clone();
    assert_eq!(read["error"], SupplyStop::MonthlyCap.message(), "{read}");
    // The consultation counted: the second call was never made.
    assert_eq!(model.remaining(), 1);
    let roles: Vec<String> = charged(&api, run).await.into_iter().map(|c| c.0).collect();
    assert_eq!(roles, ["main", "advisor"]);
    spent(&owner, SupplyFormula::Capped, 0).await;
    leave(&api, &owner).await;
}
