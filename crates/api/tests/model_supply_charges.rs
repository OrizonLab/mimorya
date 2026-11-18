//! A run is charged by the company's models it took for its execution
//! (ADR-208): whatever the operator changes of the company's supply while
//! it runs (its markup, moving it to a prepaid credit), each of its calls,
//! made on the operator's key, is counted under the formula and at the
//! markup the run started with; the change serves the runs after. Taking
//! the platform's model away from the company stops the run before its
//! next call (step 1), what its first call cost counted as it was taken.

mod support;

use std::time::Duration;

use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::provisioning::{CapChoice, SupplyChange, change_model_supply};
use mimorya_api::services::{LlmService, llm::Supply};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::{SupplyFormula, SupplyStop};
use sqlx::PgPool;
use support::llm::{MASTER_KEY, SCRIPTED_CALL_MICROS};
use support::runs::start_run;
use support::{Client, TestApi, owner_url};
use tokio::runtime::Handle;
use tokio::task::block_in_place;
use uuid::Uuid;

const CAPPED: Supply = Supply {
    formula: SupplyFormula::Capped,
    markup_bps: 0,
};

/// Gives the company `to`, and a credit when it is a prepaid one.
async fn supply(owner: &PgPool, to: Supply) {
    let changes = [
        SupplyChange::Formula {
            formula: to.formula,
            cap: CapChoice::Default,
        },
        SupplyChange::Markup { bps: to.markup_bps },
    ];
    for change in changes {
        change_model_supply(owner, tenant::SLUG, change)
            .await
            .unwrap();
    }
    if to.formula == SupplyFormula::Prepaid {
        let credit = SupplyChange::Credit {
            amount_micros: 10_000_000,
            reference: "FAC-2026-118".into(),
        };
        change_model_supply(owner, tenant::SLUG, credit)
            .await
            .unwrap();
    }
}

/// Waits until `llm` loaded the company's supply `to` again.
async fn loaded(llm: &LlmService, to: Supply) {
    for _ in 0..100 {
        if llm.resolve(tenant::ID).await.unwrap().supply == to {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the company's supply was never loaded again");
}

/// What the calls of `run` were charged: formula and re-invoiced amount.
async fn charged(api: &TestApi, run: Uuid) -> Vec<(String, Option<i64>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select supply_formula, billed_micros from usage_records \
         where run_id = $1 and kind = 'llm' order by id",
    )
    .bind(run)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// Starts a run whose first call is made while the operator changes the
/// company's supply to `to`, every process loading it again; the answer,
/// empty, is refused by the review, and the run would ask again.
async fn run_changing_to(api: &TestApi, owner: &PgPool, marc: &Client<'_>, to: Supply) -> Uuid {
    let (llm, changing) = (api.app.services.llm.clone(), owner.clone());
    api.script(vec![
        ScriptedStep::dynamic(move |_| {
            block_in_place(|| {
                Handle::current().block_on(async {
                    supply(&changing, to).await;
                    loaded(&llm, to).await;
                });
            });
            ScriptedReply::text("")
        }),
        ScriptedStep::text("Trois interventions cette semaine."),
    ]);
    let run = start_run(marc).await;
    api.idle().await;
    // Back on the cap for what follows.
    supply(owner, CAPPED).await;
    loaded(&api.app.services.llm, CAPPED).await;
    run
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires PostgreSQL"]
async fn a_run_is_charged_by_the_models_it_took_whatever_changes_meanwhile() {
    // Each call of the scripted model costs `SCRIPTED_CALL_MICROS`.
    let api = TestApi::start_priced_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    let changes = [
        Supply {
            markup_bps: 1_000,
            ..CAPPED
        },
        Supply {
            formula: SupplyFormula::Prepaid,
            markup_bps: 0,
        },
    ];
    for to in changes {
        let run = run_changing_to(&api, &owner, &marc, to).await;
        let capped = ("capped".to_string(), Some(SCRIPTED_CALL_MICROS));
        assert_eq!(charged(&api, run).await, [capped.clone(), capped], "{to:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires PostgreSQL"]
async fn taking_the_platform_s_model_away_stops_a_run_before_its_next_call() {
    let api = TestApi::start_priced_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    let withdrawn = Supply {
        formula: SupplyFormula::ClientKey,
        markup_bps: 0,
    };
    let run = run_changing_to(&api, &owner, &marc, withdrawn).await;
    let read = marc.get(&format!("/runs/{run}")).await.json()["run"].clone();
    assert_eq!(read["status"], "failed", "{read}");
    assert_eq!(read["error"], SupplyStop::Withdrawn.message());
    let capped = ("capped".to_string(), Some(SCRIPTED_CALL_MICROS));
    assert_eq!(charged(&api, run).await, [capped]);
    assert_eq!(api.model.as_ref().unwrap().requests().len(), 1);
}
