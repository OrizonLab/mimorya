//! When the operator's supply of a company's model is spent (ADR-208, step
//! 1): its monthly cap reached, its prepaid credit used up, or no formula
//! of the operator's left to it. No call is then made on the operator's
//! key: no run is launched, and a run under way stops before its next call
//! to the model, its user told why, in French.

use super::SupplyFormula;

/// What a company's supply stands at, as read before a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupplyState {
    pub formula: SupplyFormula,
    pub monthly_cap_micros: Option<i64>,
    pub unlimited: bool,
    /// What the calls under the capped formula were re-invoiced this month.
    pub capped_month_billed_micros: i64,
    /// The prepaid credit's balance: payments less prepaid consumption.
    pub prepaid_balance_micros: i64,
}

/// Why no call may be made on the operator's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyStop {
    MonthlyCap,
    CreditSpent,
    /// The operator no longer supplies the company.
    Withdrawn,
}

impl SupplyStop {
    /// As journaled, without anything else.
    pub fn reason(self) -> &'static str {
        match self {
            Self::MonthlyCap => "monthly_cap",
            Self::CreditSpent => "credit_spent",
            Self::Withdrawn => "withdrawn",
        }
    }

    /// What the person served is told.
    pub fn message(self) -> &'static str {
        match self {
            Self::MonthlyCap => {
                "Le plafond mensuel de consommation du modèle de votre entreprise est atteint : \
                 contactez votre administrateur."
            }
            Self::CreditSpent => {
                "Le crédit de consommation du modèle de votre entreprise est épuisé : contactez \
                 votre administrateur."
            }
            Self::Withdrawn => {
                "Le modèle de la plateforme n’est plus fourni à votre entreprise : contactez \
                 votre administrateur."
            }
        }
    }
}

/// Why a call on the operator's key may not be made, `state` being what it
/// is; `None` while it may. A cap is reached once what the month was
/// re-invoiced comes to it; a credit is spent once its balance is nought or
/// less.
pub fn supply_stop(state: &SupplyState) -> Option<SupplyStop> {
    match state.formula {
        SupplyFormula::ClientKey => Some(SupplyStop::Withdrawn),
        SupplyFormula::Capped if state.unlimited => None,
        SupplyFormula::Capped => {
            // Without a cap, which the table forbids, nothing is spent.
            let cap = state.monthly_cap_micros.unwrap_or(0);
            (state.capped_month_billed_micros >= cap).then_some(SupplyStop::MonthlyCap)
        }
        SupplyFormula::Prepaid => {
            (state.prepaid_balance_micros <= 0).then_some(SupplyStop::CreditSpent)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(formula: SupplyFormula) -> SupplyState {
        SupplyState {
            formula,
            monthly_cap_micros: None,
            unlimited: false,
            capped_month_billed_micros: 0,
            prepaid_balance_micros: 0,
        }
    }

    #[test]
    fn a_cap_is_reached_once_the_month_comes_to_it() {
        let capped = |billed: i64| SupplyState {
            monthly_cap_micros: Some(100_000_000),
            capped_month_billed_micros: billed,
            ..state(SupplyFormula::Capped)
        };
        assert_eq!(supply_stop(&capped(99_999_999)), None);
        assert_eq!(
            supply_stop(&capped(100_000_000)),
            Some(SupplyStop::MonthlyCap)
        );
        assert_eq!(
            supply_stop(&capped(150_000_000)),
            Some(SupplyStop::MonthlyCap)
        );
        let unlimited = SupplyState {
            unlimited: true,
            capped_month_billed_micros: i64::MAX,
            ..state(SupplyFormula::Capped)
        };
        assert_eq!(supply_stop(&unlimited), None);
        assert_eq!(
            supply_stop(&state(SupplyFormula::Capped)),
            Some(SupplyStop::MonthlyCap)
        );
    }

    #[test]
    fn a_credit_is_spent_once_its_balance_is_nought() {
        let prepaid = |balance: i64| SupplyState {
            prepaid_balance_micros: balance,
            ..state(SupplyFormula::Prepaid)
        };
        assert_eq!(supply_stop(&prepaid(1)), None);
        assert_eq!(supply_stop(&prepaid(0)), Some(SupplyStop::CreditSpent));
        assert_eq!(supply_stop(&prepaid(-5)), Some(SupplyStop::CreditSpent));
    }

    #[test]
    fn a_company_no_longer_supplied_may_not_use_the_operator_s_key() {
        assert_eq!(
            supply_stop(&state(SupplyFormula::ClientKey)),
            Some(SupplyStop::Withdrawn)
        );
        for stop in [
            SupplyStop::MonthlyCap,
            SupplyStop::CreditSpent,
            SupplyStop::Withdrawn,
        ] {
            assert!(stop.message().ends_with("contactez votre administrateur."));
        }
    }
}
