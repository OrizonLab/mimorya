//! The operator's supply of the company's model, as those who manage its
//! settings read it (`settings:manage`, company-wide, ADR-210): read only,
//! the operator alone changing it (ADR-208).

use axum::Json;
use axum::extract::State;
use mimorya_core::model_supply::SupplyFormula;

use crate::app::AppState;
use crate::contract::admin::{CreditPayment, ModelSupplyView, SupplyFormulaView, SupplyRunsView};
use crate::http::{ApiResult, iso};
use crate::plugins::Auth;
use crate::services::admin::{RunsOn, company_model_supply};

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<ModelSupplyView>> {
    let access = state.access.context(identity).await?;
    let overview = company_model_supply(&state.db, &state.llm, &access).await?;
    let supply = overview.state;
    let formula = match supply.formula {
        SupplyFormula::ClientKey => SupplyFormulaView::ClientKey,
        SupplyFormula::Capped => SupplyFormulaView::Capped,
        SupplyFormula::Prepaid => SupplyFormulaView::Prepaid,
    };
    let runs = match overview.runs {
        RunsOn::OperatorKey => SupplyRunsView::OperatorKey,
        RunsOn::OwnProvider => SupplyRunsView::OwnProvider,
        RunsOn::Nothing => SupplyRunsView::None,
    };
    Ok(Json(ModelSupplyView {
        formula,
        runs,
        monthly_cap_micros: supply.monthly_cap_micros,
        unlimited: supply.unlimited,
        month_start: iso(&overview.month_start),
        month_billed_micros: supply.capped_month_billed_micros,
        balance_micros: overview.balance_micros,
        payments: overview
            .payments
            .iter()
            .map(|payment| CreditPayment {
                amount_micros: payment.amount_micros,
                reference: payment.reference.clone(),
                recorded_at: iso(&payment.recorded_at),
            })
            .collect(),
        stopped: overview.stopped.map(|stop| stop.message().to_string()),
    }))
}
