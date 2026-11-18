//! A run is charged by the company's models it took for its execution
//! (ADR-208): whatever the operator changes of the company's supply while
//! it runs (taking away its platform model, moving it to a prepaid credit),
//! each of its calls, made on the operator's key, is counted under the
//! formula and at the markup the run started with; the change serves the
//! runs after.

mod support;

use std::time::Duration;

use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::LlmService;
use mimorya_api::services::provisioning::{CapChoice, SupplyChange, change_model_supply};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::SupplyFormula;
use sqlx::PgPool;
use support::llm::{MASTER_KEY, SCRIPTED_CALL_MICROS};
use support::runs::start_run;
use support::{TestApi, owner_url};
use tokio::runtime::Handle;
use tokio::task::block_in_place;
use uuid::Uuid;

async fn supply(owner: &PgPool, formula: SupplyFormula) {
    let change = SupplyChange::Formula {
        formula,
        cap: CapChoice::Default,
    };
    change_model_supply(owner, tenant::SLUG, change)
        .await
        .unwrap();
}

/// Waits until `llm` loaded the company's formula `formula` again.
async fn loaded(llm: &LlmService, formula: SupplyFormula) {
    for _ in 0..100 {
        if llm.resolve(tenant::ID).await.unwrap().supply.formula == formula {
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires PostgreSQL"]
async fn a_run_is_charged_by_the_models_it_took_whatever_changes_meanwhile() {
    // Each call of the scripted model costs `SCRIPTED_CALL_MICROS`.
    let api = TestApi::start_priced_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    for changed in [SupplyFormula::ClientKey, SupplyFormula::Prepaid] {
        let (llm, changing) = (api.app.services.llm.clone(), owner.clone());
        api.script(vec![
            // While the run's first call is made, the operator changes the
            // company's supply, every process loading it again; the answer,
            // empty, is refused by the review, and the run asks again.
            ScriptedStep::dynamic(move |_| {
                block_in_place(|| {
                    Handle::current().block_on(async {
                        supply(&changing, changed).await;
                        loaded(&llm, changed).await;
                    });
                });
                ScriptedReply::text("")
            }),
            ScriptedStep::text("Trois interventions cette semaine."),
        ]);
        let run = start_run(&marc).await;
        api.idle().await;
        let capped = ("capped".to_string(), Some(SCRIPTED_CALL_MICROS));
        assert_eq!(
            charged(&api, run).await,
            [capped.clone(), capped],
            "{changed:?}"
        );
        // Back on the cap for what follows.
        supply(&owner, SupplyFormula::Capped).await;
        loaded(&api.app.services.llm, SupplyFormula::Capped).await;
    }
}
