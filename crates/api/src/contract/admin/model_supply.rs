//! The operator's supply of the company's model as its administrators read
//! it (ADR-210): its formula, its monthly cap and what the month was
//! re-invoiced under it, or its prepaid credit's balance and payments. What
//! the company pays, never the operator's real cost nor its markup.

use serde::Serialize;
use ts_rs::TS;

/// How the company's model is supplied (ADR-208).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SupplyFormulaView {
    /// Its own key: the operator supplies nothing.
    ClientKey,
    /// The operator's key, within a monthly cap.
    Capped,
    /// The operator's key, against a prepaid credit.
    Prepaid,
}

/// A payment of the prepaid credit, as the operator recorded it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreditPayment {
    pub amount_micros: i64,
    /// The invoice it was paid against.
    pub reference: String,
    pub recorded_at: String,
}

/// `GET /admin/model-supply`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelSupplyView {
    pub formula: SupplyFormulaView,
    /// Whether the company's runs go to the operator's key now: on its own
    /// provider, they do not, whatever its formula.
    pub runs_on_operator_key: bool,
    /// The monthly cap, in micro-dollars, under the capped formula.
    pub monthly_cap_micros: Option<i64>,
    /// Without a cap, at the operator's explicit asking.
    pub unlimited: bool,
    /// The start of the month, in the platform's time zone (RFC 3339).
    pub month_start: String,
    /// What the calls under the cap were re-invoiced this month.
    pub month_billed_micros: i64,
    /// The prepaid credit's balance: payments less what was consumed of it,
    /// once the company has a credit.
    pub balance_micros: Option<i64>,
    /// The credit's payments, the latest first.
    pub payments: Vec<CreditPayment>,
    /// Why no call goes to the operator's key now, if so (in French).
    pub stopped: Option<String>,
}
